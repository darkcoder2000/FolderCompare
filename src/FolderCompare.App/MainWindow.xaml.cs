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
        if (_vm.CompareOnStartup && _vm.CompareCommand.CanExecute(null))
            await _vm.CompareCommand.ExecuteAsync(null);
    }

    private void Window_Closing(object? sender, CancelEventArgs e)
    {
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
        if (FindAncestor<ListViewItem>(e.OriginalSource as DependencyObject)?.DataContext is RowViewModel { IsExpandable: true } row)
            row.IsExpanded = !row.IsExpanded;
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
