using System.Windows.Input;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using Avalonia.VisualTree;
using FolderCompare.App.Services;
using FolderCompare.App.ViewModels;
using FolderCompare.App.Views;

namespace FolderCompare.App;

public partial class MainWindow : Window, ISelectionHost
{
    private readonly MainViewModel _vm = null!;
    private bool _syncingSelection;
    private bool _closeConfirmed;

    /// <summary>Ctrl on Windows and Linux, Cmd on macOS.</summary>
    private readonly KeyModifiers _command;

    public MainWindow() => InitializeComponent();

    public MainWindow(MainViewModel viewModel) : this()
    {
        _vm = viewModel;
        _vm.SelectionHost = this;
        DataContext = _vm;
        _command = OperatingSystem.IsMacOS() ? KeyModifiers.Meta : KeyModifiers.Control;
        ApplyShortcutTexts();
        WindowPlacement.Restore(this, _vm.Settings.Window, 200, 150);

        AddHandler(KeyDownEvent, Window_KeyDown, RoutingStrategies.Tunnel);
        ResultList.AddHandler(KeyDownEvent, ResultList_KeyDown, RoutingStrategies.Tunnel);
        ResultList.SelectionChanged += ResultList_SelectionChanged;
        ResultList.DoubleTapped += ResultList_DoubleTapped;
        ResultList.Sorting += (_, e) =>
        {
            // The tree is sorted by the view model (folders stay together); the grid must not sort the flat rows.
            e.Handled = true;
            if (e.Column.Tag is string tag && Enum.TryParse<SortKey>(tag, out var key)) _vm.SortBy(key);
        };
        foreach (var box in new[] { LeftPathBox, RightPathBox })
        {
            box.AddHandler(DragDrop.DragOverEvent, PathBox_DragOver);
            box.AddHandler(DragDrop.DropEvent, PathBox_Drop);
        }

        Opened += Window_Opened;
        Closing += Window_Closing;
    }

    private string Shortcut(string key) => (OperatingSystem.IsMacOS() ? "Cmd+" : "Ctrl+") + key;

    private void ApplyShortcutTexts()
    {
        ExportMenu.InputGesture = new KeyGesture(Key.E, _command);
        CopyRightMenu.InputGesture = new KeyGesture(Key.Right, _command);
        CopyLeftMenu.InputGesture = new KeyGesture(Key.Left, _command);
        ToolTip.SetTip(CopyRightButton, $"Copy selection to the right ({Shortcut("Right")})");
        ToolTip.SetTip(CopyLeftButton, $"Copy selection to the left ({Shortcut("Left")})");
        ToolTip.SetTip(SearchBox, $"Filter by file or folder name ({Shortcut("F")})");
    }

    // ---- ISelectionHost ------------------------------------------------------------------

    public void SelectRows(IReadOnlyCollection<RowViewModel> rows)
    {
        _syncingSelection = true;
        try
        {
            var selected = ResultList.SelectedItems;
            selected.Clear();
            foreach (var row in rows) selected.Add(row);
        }
        finally
        {
            _syncingSelection = false;
        }
        _vm.UpdateSelection(ResultList.SelectedItems.Cast<RowViewModel>().ToList(), Array.Empty<object>());
        var first = rows.FirstOrDefault();
        if (first is not null) ResultList.ScrollIntoView(first, null);
    }

    public void FocusSearch()
    {
        SearchBox.Focus();
        SearchBox.SelectAll();
    }

    private void ResultList_SelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (_syncingSelection)
        {
            // Bulk selection: the view model's set is rebuilt once afterwards (see SelectRows).
            _vm.UpdateSelection(Array.Empty<object>(), e.RemovedItems);
            return;
        }
        _vm.UpdateSelection(e.AddedItems, e.RemovedItems);
    }

    // ---- Window lifetime --------------------------------------------------------------------

    private async void Window_Opened(object? sender, EventArgs e)
    {
        WindowPlacement.FitToWorkArea(this);
        RestoreColumnWidths(_vm.Settings);
        InitColumnFitting();
        if (_vm.CompareOnStartup && _vm.CompareCommand.CanExecute(null))
            await _vm.CompareCommand.ExecuteAsync(null);
    }

    private async void Window_Closing(object? sender, WindowClosingEventArgs e)
    {
        if (_closeConfirmed) return;

        // Asking about unsaved text compare windows is asynchronous: cancel now, close again when answered.
        e.Cancel = true;
        if (!await _vm.ConfirmCloseTextComparesAsync()) return;

        _vm.Settings.Window = WindowPlacement.Save(this, _vm.Settings.Window);
        _vm.Settings.ColumnWidths = ResultList.Columns.Select(c => c.ActualWidth).ToList();
        _vm.SaveSettings();
        _closeConfirmed = true;
        Close();
    }

    private void RestoreColumnWidths(AppSettings settings)
    {
        if (settings.ColumnWidths is not { } widths || widths.Count != ResultList.Columns.Count) return;
        for (int i = 0; i < widths.Count; i++)
            if (widths[i] > 10) ResultList.Columns[i].Width = new DataGridLength(widths[i]);
    }

    // ---- Column layout: fill the window, status column under the swap button ---------------

    // Column order in MainWindow.axaml: left name, size, date | status | right name, size, date.
    private const int LeftNameColumn = 0, StatusColumn = 3, RightNameColumn = 4;
    private const double MinNameWidth = 120;
    // Keeps the rows a few pixels narrower than the viewport so no horizontal scrollbar appears.
    private const double RightReserve = 4;

    private bool _columnFitPending, _fittingColumns, _pointerDown;
    private ScrollBar? _verticalScrollBar;

    /// <summary>
    /// The two name columns are sized automatically: together the columns fill the grid's width and the status
    /// column (≠, =, →, ...) sits centered under the swap button. Size and date columns stay user-resizable;
    /// the name columns absorb the change.
    /// </summary>
    private void InitColumnFitting()
    {
        ResultList.Columns[LeftNameColumn].CanUserResize = false;
        ResultList.Columns[RightNameColumn].CanUserResize = false;
        foreach (var column in ResultList.Columns)
            column.PropertyChanged += (_, e) =>
            {
                if (e.Property == DataGridColumn.WidthProperty) ScheduleColumnFit();
            };

        // While a column is being resized, re-fitting would move the dragged edge; fit once the mouse is released.
        ResultList.AddHandler(PointerPressedEvent, (_, _) => _pointerDown = true, RoutingStrategies.Tunnel, handledEventsToo: true);
        ResultList.AddHandler(PointerReleasedEvent, (_, _) =>
        {
            _pointerDown = false;
            ScheduleColumnFit();
        }, RoutingStrategies.Tunnel, handledEventsToo: true);

        ResultList.SizeChanged += (_, _) => ScheduleColumnFit();
        // The swap button moves when the window width changes.
        SwapButton.PropertyChanged += (_, e) =>
        {
            if (e.Property == BoundsProperty) ScheduleColumnFit();
        };
        ScheduleColumnFit();
    }

    private void ScheduleColumnFit()
    {
        if (_fittingColumns || _pointerDown || _columnFitPending) return;
        _columnFitPending = true;
        Dispatcher.UIThread.Post(() =>
        {
            _columnFitPending = false;
            FitColumns();
        }, DispatcherPriority.Background);
    }

    private void FitColumns()
    {
        var c = ResultList.Columns;
        if (c.Count != 7 || !SwapButton.IsVisible || ResultList.Bounds.Width <= 0) return;
        _verticalScrollBar ??= ResultList.GetVisualDescendants().OfType<ScrollBar>()
            .FirstOrDefault(s => s.Orientation == Avalonia.Layout.Orientation.Vertical);

        double status = c[StatusColumn].ActualWidth;
        double leftFixed = c[1].ActualWidth + c[2].ActualWidth;
        double rightFixed = c[5].ActualWidth + c[6].ActualWidth;

        // Positions relative to the grid's left edge, where the first column starts.
        double scrollBar = _verticalScrollBar is { IsVisible: true } bar ? bar.Bounds.Width : 0;
        double available = ResultList.Bounds.Width - scrollBar - RightReserve;
        var swapCenter = SwapButton.TranslatePoint(new Point(SwapButton.Bounds.Width / 2, 0), ResultList);
        if (swapCenter is null) return;

        double leftName = swapCenter.Value.X - status / 2 - leftFixed;
        double rightName = available - (swapCenter.Value.X + status / 2) - rightFixed;
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

    private static void SetColumnWidth(DataGridColumn column, double width)
    {
        width = Math.Floor(width);
        if (Math.Abs(column.ActualWidth - width) >= 1) column.Width = new DataGridLength(width);
    }

    private void Exit_Click(object? sender, RoutedEventArgs e) => Close();

    // ---- Keyboard ------------------------------------------------------------------------------

    private void Window_KeyDown(object? sender, KeyEventArgs e)
    {
        var mods = e.KeyModifiers;
        switch (e.Key)
        {
            case Key.F5 when mods == KeyModifiers.None:
                Execute(_vm.CompareCommand, e);
                return;
            case Key.Escape when _vm.IsBusy:
                Execute(_vm.CancelCommand, e);
                return;
            case Key.F when mods == _command:
                FocusSearch();
                e.Handled = true;
                return;
            case Key.E when mods == _command:
                Execute(_vm.ExportReportCommand, e);
                return;
        }

        // The remaining shortcuts would conflict with text editing.
        if (FocusManager?.GetFocusedElement() is TextBox) return;

        switch (e.Key)
        {
            case Key.Right when mods == _command:
                Execute(_vm.CopyToRightCommand, e);
                break;
            case Key.Left when mods == _command:
                Execute(_vm.CopyToLeftCommand, e);
                break;
            case Key.Delete when mods == KeyModifiers.None:
            case Key.Back when mods == KeyModifiers.Meta && OperatingSystem.IsMacOS():
                Execute(_vm.DeleteCommand, e);
                break;
            case Key.F2 when mods == KeyModifiers.None:
                Execute(_vm.RenameCommand, e);
                break;
            case Key.A when mods == _command:
                Execute(_vm.SelectAllCommand, e);
                break;
        }
    }

    private static void Execute(ICommand command, KeyEventArgs e)
    {
        if (command.CanExecute(null)) command.Execute(null);
        e.Handled = true;
    }

    private void ResultList_KeyDown(object? sender, KeyEventArgs e)
    {
        if (e.KeyModifiers != KeyModifiers.None) return;
        if (FocusedRow() is not { } row) return;

        switch (e.Key)
        {
            case Key.Right when row.IsExpandable && !row.IsExpanded:
                row.IsExpanded = true;
                e.Handled = true;
                break;
            case Key.Right:
                e.Handled = true; // no horizontal cell navigation
                break;
            case Key.Left when row.IsExpanded:
                row.IsExpanded = false;
                e.Handled = true;
                break;
            case Key.Left when row.Node.Parent is { } parent:
                var parentRow = _vm.Rows.FirstOrDefault(r => ReferenceEquals(r.Node, parent));
                if (parentRow is not null) FocusRow(parentRow);
                e.Handled = true;
                break;
            case Key.Left:
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
            case Key.Enter:
                e.Handled = true;
                break;
        }
    }

    private RowViewModel? FocusedRow() =>
        ((FocusManager?.GetFocusedElement() as Visual)?.FindAncestorOfType<DataGridRow>(includeSelf: true)?.DataContext
         ?? ResultList.SelectedItem) as RowViewModel;

    private void FocusRow(RowViewModel row)
    {
        ResultList.ScrollIntoView(row, null);
        ResultList.SelectedItem = row;
    }

    // ---- Mouse --------------------------------------------------------------------------------

    private void ResultList_DoubleTapped(object? sender, TappedEventArgs e)
    {
        if (e.Source is Visual source && source.FindAncestorOfType<ToggleButton>(includeSelf: true) is not null) return;
        if (e.Source is Visual v && v.FindAncestorOfType<DataGridColumnHeader>(includeSelf: true) is not null) return;
        if ((e.Source as StyledElement)?.DataContext is not RowViewModel row) return;
        if (row.IsExpandable) row.IsExpanded = !row.IsExpanded;
        else if (!row.Node.IsDirectory && _vm.CompareContentsCommand.CanExecute(null))
        {
            _vm.CompareContentsCommand.Execute(null);
            e.Handled = true;
        }
    }

    // ---- Drag and drop of folders onto the path boxes -------------------------------------------

    private void PathBox_DragOver(object? sender, DragEventArgs e)
    {
        e.DragEffects = DroppedFolder(e) is not null ? DragDropEffects.Copy : DragDropEffects.None;
        e.Handled = true;
    }

    private void PathBox_Drop(object? sender, DragEventArgs e)
    {
        var folder = DroppedFolder(e);
        if (folder is null || sender is not ComboBox { Tag: string side }) return;
        if (side == "Left") _vm.LeftPath = folder;
        else _vm.RightPath = folder;
        e.Handled = true;
    }

    private static string? DroppedFolder(DragEventArgs e)
    {
        var path = e.DataTransfer.TryGetFiles()?.FirstOrDefault()?.TryGetLocalPath();
        if (path is null) return null;
        if (Directory.Exists(path)) return path;
        return File.Exists(path) ? Path.GetDirectoryName(path) : null;
    }
}
