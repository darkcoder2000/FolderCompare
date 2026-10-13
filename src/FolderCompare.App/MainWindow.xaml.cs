using System.ComponentModel;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Media;
using FolderCompare.App.Services;
using FolderCompare.App.ViewModels;

namespace FolderCompare.App;

public partial class MainWindow : Window, ISelectionHost
{
    private readonly MainViewModel _vm;
    private bool _syncingSelection;

    public MainWindow(MainViewModel viewModel)
    {
        InitializeComponent();
        _vm = viewModel;
        _vm.SelectionHost = this;
        DataContext = _vm;
        RestorePlacement(_vm.Settings);
    }

    // ---- ISelectionHost ------------------------------------------------------------------

    public void SelectRows(IReadOnlyCollection<RowViewModel> rows)
    {
        _syncingSelection = true;
        try
        {
            ResultList.SelectItems(rows);
        }
        finally
        {
            _syncingSelection = false;
        }
        _vm.UpdateSelection(ResultList.SelectedItems, Array.Empty<object>());
        var first = rows.FirstOrDefault();
        if (first is not null) ResultList.ScrollIntoView(first);
    }

    public void FocusSearch()
    {
        SearchBox.Focus();
        SearchBox.SelectAll();
    }

    private void ResultList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncingSelection)
        {
            // Bulk selection: rebuild the view model's set once afterwards (see SelectRows).
            _vm.UpdateSelection(Array.Empty<object>(), e.RemovedItems);
            return;
        }
        _vm.UpdateSelection(e.AddedItems, e.RemovedItems);
    }

    // ---- Window lifetime --------------------------------------------------------------------

    private async void Window_Loaded(object sender, RoutedEventArgs e)
    {
        RestoreColumnWidths(_vm.Settings);
        InitColumnFitting();
        if (_vm.CompareOnStartup && _vm.CompareCommand.CanExecute(null))
            await _vm.CompareCommand.ExecuteAsync(null);
    }

    private void Window_Closing(object? sender, CancelEventArgs e)
    {
        if (!_vm.ConfirmCloseTextCompares())
        {
            e.Cancel = true;
            return;
        }

        var s = _vm.Settings;
        var bounds = WindowState == WindowState.Normal ? new Rect(Left, Top, Width, Height) : RestoreBounds;
        s.Window = new WindowSettings
        {
            Left = bounds.Left,
            Top = bounds.Top,
            Width = bounds.Width,
            Height = bounds.Height,
            Maximized = WindowState == WindowState.Maximized,
        };
        if (ResultList.View is GridView grid) s.ColumnWidths = grid.Columns.Select(c => c.ActualWidth).ToList();
        _vm.SaveSettings();
    }

    private void RestorePlacement(AppSettings settings)
    {
        SourceInitialized += (_, _) => FitToWorkArea();

        var w = settings.Window;
        if (w is null || w.Width < 200 || w.Height < 150) return;
        var virtualScreen = new Rect(SystemParameters.VirtualScreenLeft, SystemParameters.VirtualScreenTop,
                                     SystemParameters.VirtualScreenWidth, SystemParameters.VirtualScreenHeight);
        if (!virtualScreen.IntersectsWith(new Rect(w.Left, w.Top, w.Width, w.Height))) return;
        WindowStartupLocation = WindowStartupLocation.Manual;
        Left = w.Left;
        Top = w.Top;
        Width = w.Width;
        Height = w.Height;
        if (w.Maximized) WindowState = WindowState.Maximized;
    }

    /// <summary>Shrinks and moves the window so it fits on the monitor's work area (in this window's DPI).</summary>
    private void FitToWorkArea()
    {
        if (WindowState != WindowState.Normal) return;
        var handle = new System.Windows.Interop.WindowInteropHelper(this).Handle;
        var monitor = NativeMethods.MonitorFromWindow(handle, NativeMethods.MonitorDefaultToNearest);
        var info = new NativeMethods.MonitorInfo { Size = System.Runtime.InteropServices.Marshal.SizeOf<NativeMethods.MonitorInfo>() };
        if (!NativeMethods.GetMonitorInfo(monitor, ref info)) return;

        var dpi = VisualTreeHelper.GetDpi(this);
        var work = new Rect(info.Work.Left / dpi.DpiScaleX, info.Work.Top / dpi.DpiScaleY,
                            (info.Work.Right - info.Work.Left) / dpi.DpiScaleX, (info.Work.Bottom - info.Work.Top) / dpi.DpiScaleY);
        Width = Math.Min(Width, work.Width);
        Height = Math.Min(Height, work.Height);
        if (!double.IsNaN(Left)) Left = Math.Clamp(Left, work.Left, Math.Max(work.Left, work.Right - Width));
        if (!double.IsNaN(Top)) Top = Math.Clamp(Top, work.Top, Math.Max(work.Top, work.Bottom - Height));
    }

    private static class NativeMethods
    {
        public const uint MonitorDefaultToNearest = 2;

        [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
        public struct NativeRect
        {
            public int Left, Top, Right, Bottom;
        }

        [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
        public struct MonitorInfo
        {
            public int Size;
            public NativeRect Monitor;
            public NativeRect Work;
            public uint Flags;
        }

        [System.Runtime.InteropServices.DllImport("user32.dll")]
        public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);

        [System.Runtime.InteropServices.DllImport("user32.dll")]
        public static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
    }

    private void RestoreColumnWidths(AppSettings settings)
    {
        if (settings.ColumnWidths is not { } widths || ResultList.View is not GridView grid || widths.Count != grid.Columns.Count) return;
        for (int i = 0; i < widths.Count; i++)
            if (widths[i] > 10) grid.Columns[i].Width = widths[i];
    }

    // ---- Column layout: fill the window, status column under the swap button ---------------

    // Column order in MainWindow.xaml: left name, size, date | status | right name, size, date.
    private const int LeftNameColumn = 0, StatusColumn = 3, RightNameColumn = 4;
    private const double MinNameWidth = 120;
    // Keeps the rows a few pixels narrower than the viewport (item borders, header margin) so no
    // horizontal scrollbar appears.
    private const double RightReserve = 6;

    private ScrollViewer? _resultScroll;
    private GridViewHeaderRowPresenter? _headerPresenter;
    private bool _columnFitPending;
    private bool _fittingColumns;
    private bool _resizingColumn;
    private bool _grippersLocked;

    /// <summary>
    /// The two name columns are sized automatically: together the columns fill the list's width and the
    /// status column (≠, =, →, ...) sits centered under the ⇄ swap button. Size and date columns stay
    /// user-resizable; the name columns absorb the change.
    /// </summary>
    private void InitColumnFitting()
    {
        if (ResultList.View is not GridView grid) return;

        var widthDescriptor = DependencyPropertyDescriptor.FromProperty(GridViewColumn.WidthProperty, typeof(GridViewColumn));
        foreach (var column in grid.Columns)
            widthDescriptor.AddValueChanged(column, (_, _) => ScheduleColumnFit());

        // The swap button only moves when the window width changes (DPI changes resize the window, too).
        SizeChanged += (_, _) => ScheduleColumnFit();
        ResultList.AddHandler(ScrollViewer.ScrollChangedEvent, new ScrollChangedEventHandler((_, e) =>
        {
            // The vertical scrollbar appearing/disappearing changes the usable width.
            if (e.ViewportWidthChange != 0) ScheduleColumnFit();
        }));

        // While a header gripper is dragged, re-fitting would move the dragged header under the mouse and the
        // thumb's relative drag delta would run away (the column collapses). Fit once when the drag ends.
        ResultList.AddHandler(Thumb.DragStartedEvent, new DragStartedEventHandler((_, e) =>
        {
            if (IsHeaderGripper(e.OriginalSource)) _resizingColumn = true;
        }), handledEventsToo: true);
        ResultList.AddHandler(Thumb.DragCompletedEvent, new DragCompletedEventHandler((_, e) =>
        {
            if (!IsHeaderGripper(e.OriginalSource)) return;
            _resizingColumn = false;
            ScheduleColumnFit();
        }), handledEventsToo: true);

        ScheduleColumnFit();
    }

    private static bool IsHeaderGripper(object source) =>
        source is Thumb { TemplatedParent: GridViewColumnHeader };

    private void ScheduleColumnFit()
    {
        if (_fittingColumns || _resizingColumn || _columnFitPending) return;
        _columnFitPending = true;
        Dispatcher.InvokeAsync(() =>
        {
            _columnFitPending = false;
            FitColumns();
        }, System.Windows.Threading.DispatcherPriority.Loaded);
    }

    private void FitColumns()
    {
        if (ResultList.View is not GridView grid || grid.Columns.Count != 7) return;
        _resultScroll ??= FindDescendant<ScrollViewer>(ResultList);
        _headerPresenter ??= FindDescendant<GridViewHeaderRowPresenter>(ResultList);
        if (_resultScroll is null || _headerPresenter is null || !SwapButton.IsVisible || _resultScroll.ViewportWidth <= 0) return;
        LockAutoSizedGrippers();

        var c = grid.Columns;
        double status = c[StatusColumn].ActualWidth;
        double leftFixed = c[1].ActualWidth + c[2].ActualWidth;
        double rightFixed = c[5].ActualWidth + c[6].ActualWidth;

        // Positions in the header row's coordinates, where the first column starts at x = 0. The header row
        // scrolls with the list, so its current position already includes the horizontal offset.
        double headerX = _headerPresenter.TranslatePoint(new Point(0, 0), _resultScroll).X;
        double available = _resultScroll.ViewportWidth - (headerX + _resultScroll.HorizontalOffset) - RightReserve;
        double swapCenter = SwapButton.TranslatePoint(new Point(SwapButton.ActualWidth / 2, 0), _resultScroll).X - headerX;

        double leftName = swapCenter - status / 2 - leftFixed;
        double rightName = available - (swapCenter + status / 2) - rightFixed;
        if (leftName < MinNameWidth || rightName < MinNameWidth)
        {
            // Too narrow to align with the button: split what is left evenly (may scroll horizontally).
            double rest = available - status - leftFixed - rightFixed;
            leftName = rightName = Math.Max(MinNameWidth, rest / 2);
        }

        _fittingColumns = true;
        try
        {
            SetColumnWidth(c[LeftNameColumn], leftName);
            SetColumnWidth(c[RightNameColumn], rightName);
        }
        finally
        {
            _fittingColumns = false;
        }
    }

    private static void SetColumnWidth(GridViewColumn column, double width)
    {
        width = Math.Floor(width);
        if (Math.Abs(column.Width - width) >= 1) column.Width = width;
    }

    /// <summary>
    /// Hides the resize grippers of the auto-sized columns (each gripper sits on its column's right edge),
    /// so dragging there does not fight the automatic layout. Size and date columns stay resizable.
    /// </summary>
    private void LockAutoSizedGrippers()
    {
        if (_grippersLocked || _headerPresenter is null || ResultList.View is not GridView grid) return;
        var autoSized = new[] { grid.Columns[LeftNameColumn], grid.Columns[StatusColumn], grid.Columns[RightNameColumn] };
        int locked = 0;
        foreach (var header in FindDescendants<GridViewColumnHeader>(_headerPresenter))
        {
            if (header.Column is null || !autoSized.Contains(header.Column)) continue;
            if (header.Template?.FindName("PART_HeaderGripper", header) is Thumb gripper)
            {
                gripper.Visibility = Visibility.Collapsed;
                locked++;
            }
        }
        _grippersLocked = locked == autoSized.Length;
    }

    private static T? FindDescendant<T>(DependencyObject parent) where T : DependencyObject =>
        FindDescendants<T>(parent).FirstOrDefault();

    private static IEnumerable<T> FindDescendants<T>(DependencyObject parent) where T : DependencyObject
    {
        for (int i = 0; i < VisualTreeHelper.GetChildrenCount(parent); i++)
        {
            var child = VisualTreeHelper.GetChild(parent, i);
            if (child is T match) yield return match;
            foreach (var nested in FindDescendants<T>(child)) yield return nested;
        }
    }

    private void Exit_Click(object sender, RoutedEventArgs e) => Close();

    // ---- Keyboard ------------------------------------------------------------------------------

    private void Window_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        var mods = Keyboard.Modifiers;
        var key = e.Key == Key.System ? e.SystemKey : e.Key;

        switch (key)
        {
            case Key.F5 when mods == ModifierKeys.None:
                Execute(_vm.CompareCommand, e);
                return;
            case Key.Escape when _vm.IsBusy:
                Execute(_vm.CancelCommand, e);
                return;
            case Key.F when mods == ModifierKeys.Control:
                FocusSearch();
                e.Handled = true;
                return;
            case Key.E when mods == ModifierKeys.Control:
                Execute(_vm.ExportReportCommand, e);
                return;
        }

        // The remaining shortcuts would conflict with text editing.
        if (Keyboard.FocusedElement is TextBox) return;

        switch (key)
        {
            case Key.Right when mods == ModifierKeys.Control:
                Execute(_vm.CopyToRightCommand, e);
                break;
            case Key.Left when mods == ModifierKeys.Control:
                Execute(_vm.CopyToLeftCommand, e);
                break;
            case Key.Delete when mods == ModifierKeys.None:
                Execute(_vm.DeleteCommand, e);
                break;
            case Key.F2 when mods == ModifierKeys.None:
                Execute(_vm.RenameCommand, e);
                break;
            case Key.A when mods == ModifierKeys.Control:
                Execute(_vm.SelectAllCommand, e);
                break;
        }
    }

    private static void Execute(ICommand command, KeyEventArgs e)
    {
        if (command.CanExecute(null)) command.Execute(null);
        e.Handled = true;
    }

    private void ResultList_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (Keyboard.Modifiers != ModifierKeys.None) return;
        if (FocusedRow() is not { } row) return;

        switch (e.Key)
        {
            case Key.Right when row.IsExpandable && !row.IsExpanded:
                row.IsExpanded = true;
                e.Handled = true;
                break;
            case Key.Left when row.IsExpanded:
                row.IsExpanded = false;
                e.Handled = true;
                break;
            case Key.Left when row.Node.Parent is { } parent:
                var parentRow = _vm.Rows.FirstOrDefault(r => ReferenceEquals(r.Node, parent));
                if (parentRow is not null) FocusRow(parentRow, select: true);
                e.Handled = true;
                break;
            case Key.Space:
                if (ResultList.SelectedItems.Contains(row)) ResultList.SelectedItems.Remove(row);
                else ResultList.SelectedItems.Add(row);
                e.Handled = true;
                break;
            case Key.Enter when row.IsExpandable:
                row.IsExpanded = !row.IsExpanded;
                e.Handled = true;
                break;
            case Key.Enter when !row.Node.IsDirectory:
                Execute(_vm.CompareContentsCommand, e);
                break;
        }
    }

    private RowViewModel? FocusedRow() =>
        (Keyboard.FocusedElement as ListViewItem)?.DataContext as RowViewModel;

    private void FocusRow(RowViewModel row, bool select)
    {
        ResultList.ScrollIntoView(row);
        if (select) ResultList.SelectedItem = row;
        if (ResultList.ItemContainerGenerator.ContainerFromItem(row) is ListViewItem item) item.Focus();
    }

    // ---- Mouse --------------------------------------------------------------------------------

    private void ResultList_MouseDoubleClick(object sender, MouseButtonEventArgs e)
    {
        if (FindAncestor<ToggleButton>(e.OriginalSource as DependencyObject) is not null) return;
        if (FindAncestor<ListViewItem>(e.OriginalSource as DependencyObject)?.DataContext is not RowViewModel row) return;
        if (row.IsExpandable) row.IsExpanded = !row.IsExpanded;
        else if (!row.Node.IsDirectory && _vm.CompareContentsCommand.CanExecute(null))
        {
            _vm.CompareContentsCommand.Execute(null);
            e.Handled = true;
        }
    }

    private void ResultList_HeaderClick(object sender, RoutedEventArgs e)
    {
        if (e.OriginalSource is GridViewColumnHeader { Tag: string tag } && Enum.TryParse<SortKey>(tag, out var key))
            _vm.SortBy(key);
    }

    private static T? FindAncestor<T>(DependencyObject? d) where T : DependencyObject
    {
        while (d is not null and not T)
            d = d is Visual or System.Windows.Media.Media3D.Visual3D ? VisualTreeHelper.GetParent(d) : LogicalTreeHelper.GetParent(d);
        return d as T;
    }

    // ---- Drag and drop of folders onto the path boxes -------------------------------------------

    private void PathBox_PreviewDragOver(object sender, DragEventArgs e)
    {
        e.Effects = DroppedFolder(e) is not null ? DragDropEffects.Copy : DragDropEffects.None;
        e.Handled = true;
    }

    private void PathBox_PreviewDrop(object sender, DragEventArgs e)
    {
        var folder = DroppedFolder(e);
        if (folder is null || sender is not ComboBox { Tag: string side }) return;
        if (side == "Left") _vm.LeftPath = folder;
        else _vm.RightPath = folder;
        e.Handled = true;
    }

    private static string? DroppedFolder(DragEventArgs e)
    {
        if (e.Data.GetData(DataFormats.FileDrop) is not string[] { Length: > 0 } paths) return null;
        var path = paths[0];
        if (Directory.Exists(path)) return path;
        return File.Exists(path) ? Path.GetDirectoryName(path) : null;
    }
}
