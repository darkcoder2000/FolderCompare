using System.Collections;
using System.Collections.ObjectModel;
using System.IO;
using System.IO.Abstractions;
using System.Text;
using System.Windows;
using System.Windows.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.Extensions.Logging;
using FolderCompare.App.Services;
using FolderCompare.Core.Comparison;
using FolderCompare.Core.Models;
using FolderCompare.Core.Operations;
using FolderCompare.Core.Reporting;

namespace FolderCompare.App.ViewModels;

/// <summary>Implemented by the view so the view model can drive ListView selection and focus.</summary>
public interface ISelectionHost
{
    void SelectRows(IReadOnlyCollection<RowViewModel> rows);
    void FocusSearch();
}

public enum SortKey
{
    Name,
    LeftSize,
    LeftDate,
    Status,
    RightSize,
    RightDate,
}

public sealed partial class MainViewModel : ObservableObject
{
    private const int MaxRecentPaths = 20;
    private const int IncrementalLimit = 200;

    private readonly IFileSystem _fs;
    private readonly DiffEngine _engine;
    private readonly FileOperationExecutor _executor;
    private readonly SettingsService _settingsService;
    private readonly IDialogService _dialogs;
    private readonly IShellService _shell;
    private readonly ITextCompareLauncher _textCompare;
    private readonly IExplorerIntegrationService _explorer;
    private readonly ILogger<MainViewModel> _logger;

    private readonly Dictionary<DiffNode, RowViewModel> _rowCache = new();
    private readonly HashSet<string> _expanded = new(StringComparer.OrdinalIgnoreCase);
    private readonly Dictionary<DiffNode, bool> _visible = new();
    private readonly HashSet<RowViewModel> _selected = new();
    private DiffResult? _result;
    private CancellationTokenSource? _cts;
    private SortKey _sortKey = SortKey.Name;
    private bool _sortAscending = true;
    private bool _suppressFilterRebuild;
    private bool _suppressProfileApply;
    private readonly DispatcherTimer _searchTimer;

    public MainViewModel(IFileSystem fs, DiffEngine engine, FileOperationExecutor executor, SettingsService settingsService,
                         IDialogService dialogs, IShellService shell, ITextCompareLauncher textCompare,
                         IExplorerIntegrationService explorer, ILogger<MainViewModel> logger)
    {
        _fs = fs;
        _engine = engine;
        _executor = executor;
        _settingsService = settingsService;
        _dialogs = dialogs;
        _shell = shell;
        _textCompare = textCompare;
        _explorer = explorer;
        _logger = logger;

        _searchTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(150) };
        _searchTimer.Tick += (_, _) =>
        {
            _searchTimer.Stop();
            RebuildRows();
        };

        RecentPaths = new ObservableCollection<string>(Settings.RecentPaths);
        Profiles = new ObservableCollection<Profile>(Settings.Profiles);
        LoadFromSettings();
        _explorerIntegration = _explorer.IsRegistered;
    }

    public AppSettings Settings => _settingsService.Current;
    public ISelectionHost? SelectionHost { get; set; }
    public RangeObservableCollection<RowViewModel> Rows { get; } = new();
    public ObservableCollection<string> RecentPaths { get; }
    public ObservableCollection<Profile> Profiles { get; }
    public DiffResult? Result => _result;

    // ---- Paths ---------------------------------------------------------------------------

    [ObservableProperty] private string _leftPath = "";
    [ObservableProperty] private string _rightPath = "";
    [ObservableProperty] private string? _leftPathError;
    [ObservableProperty] private string? _rightPathError;
    [ObservableProperty] private string? _rootError;
    [ObservableProperty] private Profile? _selectedProfile;

    partial void OnLeftPathChanged(string value) => ValidatePaths();
    partial void OnRightPathChanged(string value) => ValidatePaths();

    partial void OnSelectedProfileChanged(Profile? value)
    {
        if (value is not null && !_suppressProfileApply) ApplyProfile(value);
    }

    private void ValidatePaths()
    {
        LeftPathError = PathError(LeftPath);
        RightPathError = PathError(RightPath);
        RootError = LeftPathError is null && RightPathError is null && !string.IsNullOrWhiteSpace(LeftPath) && !string.IsNullOrWhiteSpace(RightPath)
            ? SafetyValidator.ValidateRoots(_fs, LeftPath, RightPath)
            : null;
        CompareCommand.NotifyCanExecuteChanged();
    }

    private string? PathError(string path)
    {
        if (string.IsNullOrWhiteSpace(path)) return null;
        try
        {
            return _fs.Directory.Exists(path.Trim()) ? null : "Folder does not exist or is not accessible.";
        }
        catch (Exception ex) when (ex is ArgumentException or IOException or NotSupportedException)
        {
            return "Invalid path.";
        }
    }

    [RelayCommand]
    private void Browse(string side)
    {
        bool left = side == "Left";
        var picked = _dialogs.PickFolder(left ? LeftPath : RightPath);
        if (picked is null) return;
        if (left) LeftPath = picked;
        else RightPath = picked;
    }

    [RelayCommand(CanExecute = nameof(IsIdle))]
    private async Task SwapAsync()
    {
        (LeftPath, RightPath) = (RightPath, LeftPath);
        if (_result is not null && CanCompare()) await CompareAsync();
    }

    // ---- Filters & options -----------------------------------------------------------------

    [ObservableProperty] private bool _showOnlyLeft = true;
    [ObservableProperty] private bool _showOnlyRight = true;
    [ObservableProperty] private bool _showNewerLeft = true;
    [ObservableProperty] private bool _showNewerRight = true;
    [ObservableProperty] private bool _showIdentical = true;
    [ObservableProperty] private bool _showErrors = true;
    [ObservableProperty] private bool _differencesOnly;
    [ObservableProperty] private string _searchText = "";
    [ObservableProperty] private string _excludePatterns = "";
    [ObservableProperty] private string _includePatterns = "";
    [ObservableProperty] private bool _foldersFirst = true;

    partial void OnShowOnlyLeftChanged(bool value) => FiltersChanged();
    partial void OnShowOnlyRightChanged(bool value) => FiltersChanged();
    partial void OnShowNewerLeftChanged(bool value) => FiltersChanged();
    partial void OnShowNewerRightChanged(bool value) => FiltersChanged();
    partial void OnShowIdenticalChanged(bool value) => FiltersChanged();
    partial void OnShowErrorsChanged(bool value) => FiltersChanged();
    partial void OnDifferencesOnlyChanged(bool value) => FiltersChanged();

    partial void OnSearchTextChanged(string value)
    {
        _searchTimer.Stop();
        _searchTimer.Start();
    }

    partial void OnFoldersFirstChanged(bool value)
    {
        Settings.FoldersFirst = value;
        ApplySort();
    }

    private void FiltersChanged()
    {
        if (_suppressFilterRebuild) return;
        RebuildRows();
    }

    private bool SearchActive => !string.IsNullOrWhiteSpace(SearchText);

    // ---- Busy state, progress, status -----------------------------------------------------

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsBusy))]
    [NotifyCanExecuteChangedFor(nameof(CancelCommand))]
    private bool _isScanning;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsBusy))]
    [NotifyCanExecuteChangedFor(nameof(CancelCommand))]
    private bool _isOperating;

    [ObservableProperty] private string _statusText = "Choose two folders and press Compare (F5).";
    [ObservableProperty] private string _countsText = "";
    [ObservableProperty] private string _selectionText = "";
    [ObservableProperty] private double _progressValue;
    [ObservableProperty] private bool _progressIndeterminate;
    [ObservableProperty] private double _fileProgressValue;

    public bool IsBusy => IsScanning || IsOperating;
    private bool IsIdle() => !IsBusy;

    partial void OnIsScanningChanged(bool value) => RefreshCommandStates();
    partial void OnIsOperatingChanged(bool value) => RefreshCommandStates();

    [RelayCommand(CanExecute = nameof(IsBusy))]
    private void Cancel() => _cts?.Cancel();

    // ---- Startup ---------------------------------------------------------------------------

    public bool CompareOnStartup { get; private set; }

    // --no-recursive applies to this session only and is not saved.
    private bool _forceNonRecursive;

    public void ApplyCommandLine(CommandLineOptions options)
    {
        if (options.Left is not null) LeftPath = options.Left;
        if (options.Right is not null) RightPath = options.Right;
        _forceNonRecursive = options.NoRecursive;
        CompareOnStartup = options.Compare && options.Left is not null && options.Right is not null;
    }

    private void LoadFromSettings()
    {
        _suppressFilterRebuild = true;
        LeftPath = Settings.LastLeft;
        RightPath = Settings.LastRight;
        ExcludePatterns = Settings.ExcludePatterns;
        IncludePatterns = Settings.IncludePatterns;
        FoldersFirst = Settings.FoldersFirst;
        ApplyFilters(Settings.Filters);
        _suppressFilterRebuild = false;
    }

    private void ApplyFilters(FilterSettings f)
    {
        ShowOnlyLeft = f.ShowOnlyLeft;
        ShowOnlyRight = f.ShowOnlyRight;
        ShowNewerLeft = f.ShowNewerLeft;
        ShowNewerRight = f.ShowNewerRight;
        ShowIdentical = f.ShowIdentical;
        ShowErrors = f.ShowErrors;
        DifferencesOnly = f.DifferencesOnly;
    }

    private FilterSettings CurrentFilters() => new()
    {
        ShowOnlyLeft = ShowOnlyLeft,
        ShowOnlyRight = ShowOnlyRight,
        ShowNewerLeft = ShowNewerLeft,
        ShowNewerRight = ShowNewerRight,
        ShowIdentical = ShowIdentical,
        ShowErrors = ShowErrors,
        DifferencesOnly = DifferencesOnly,
    };

    /// <summary>Copies view state into the settings object and saves it.</summary>
    public void SaveSettings()
    {
        Settings.LastLeft = LeftPath;
        Settings.LastRight = RightPath;
        Settings.ExcludePatterns = ExcludePatterns;
        Settings.IncludePatterns = IncludePatterns;
        Settings.Filters = CurrentFilters();
        Settings.FoldersFirst = FoldersFirst;
        Settings.RecentPaths = RecentPaths.ToList();
        Settings.Profiles = Profiles.ToList();
        _settingsService.Save();
    }

    // ---- Compare ---------------------------------------------------------------------------

    private bool CanCompare() => !IsBusy && !string.IsNullOrWhiteSpace(LeftPath) && !string.IsNullOrWhiteSpace(RightPath)
                                 && LeftPathError is null && RightPathError is null && RootError is null;

    [RelayCommand(CanExecute = nameof(CanCompare))]
    private async Task CompareAsync()
    {
        var error = SafetyValidator.ValidateRoots(_fs, LeftPath, RightPath);
        if (error is not null)
        {
            RootError = error;
            return;
        }

        // Reordering the recent list can clear the editable ComboBoxes' text, so capture the paths first.
        var left = LeftPath.Trim();
        var right = RightPath.Trim();
        AddRecent(right);
        AddRecent(left);
        LeftPath = left;
        RightPath = right;
        Settings.ExcludePatterns = ExcludePatterns;
        Settings.IncludePatterns = IncludePatterns;
        SaveSettings();

        _cts = new CancellationTokenSource();
        IsScanning = true;
        ProgressIndeterminate = true;
        StatusText = "Scanning...";
        var options = Settings.ToCompareOptions();
        if (_forceNonRecursive) options = options with { Recursive = false };
        var progress = new Progress<ScanProgress>(p =>
            StatusText = $"Scanning... {p.ItemsScanned:N0} items  {p.CurrentFolder}");
        var started = DateTime.UtcNow;

        try
        {
            var result = await _engine.CompareAsync(left, right, options, progress, _cts.Token);
            _result = result;
            _rowCache.Clear();
            _selected.Clear();
            SortTree(result.Root);
            RebuildRows(preserveSelection: false);
            UpdateCounts();
            var elapsed = DateTime.UtcNow - started;
            StatusText = result.Root.Status == DiffStatus.Error
                ? "Could not read a root folder: " + result.Root.ErrorMessage
                : $"Compared {result.Root.Descendants().Count():N0} items in {elapsed.TotalSeconds:0.0} s.";
            _logger.LogInformation("Compared {Left} and {Right} in {Elapsed}", result.LeftRoot, result.RightRoot, elapsed);
        }
        catch (OperationCanceledException)
        {
            StatusText = "Scan cancelled.";
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Compare failed");
            _dialogs.ShowError("Compare failed", ex.Message);
            StatusText = "Compare failed.";
        }
        finally
        {
            IsScanning = false;
            ProgressIndeterminate = false;
            ProgressValue = 0;
        }
    }

    private void AddRecent(string path)
    {
        path = path.Trim();
        var existing = RecentPaths.FirstOrDefault(p => string.Equals(p, path, StringComparison.OrdinalIgnoreCase));
        if (existing is not null) RecentPaths.Remove(existing);
        RecentPaths.Insert(0, path);
        while (RecentPaths.Count > MaxRecentPaths) RecentPaths.RemoveAt(RecentPaths.Count - 1);
    }

    // ---- Tree / rows -------------------------------------------------------------------------

    public bool IsExpanded(DiffNode node) => _expanded.Contains(node.RelativePath);

    public void SetExpanded(RowViewModel row, bool expand)
    {
        if (expand) _expanded.Add(row.Node.RelativePath);
        else _expanded.Remove(row.Node.RelativePath);
        row.NotifyExpanded();
        if (SearchActive) return;

        int index = Rows.IndexOf(row);
        if (index < 0) return;

        if (expand)
        {
            var list = new List<RowViewModel>();
            foreach (var child in row.Node.Children)
                if (_visible.TryGetValue(child, out var v) && v) Emit(child, row.Level + 1, list);
            if (list.Count == 0) return;
            if (list.Count <= IncrementalLimit)
            {
                for (int i = 0; i < list.Count; i++) Rows.Insert(index + 1 + i, list[i]);
            }
            else
            {
                var keep = _selected.ToList();
                Rows.InsertRange(index + 1, list);
                SelectionHost?.SelectRows(keep);
            }
        }
        else
        {
            int end = index + 1;
            while (end < Rows.Count && Rows[end].Level > row.Level) end++;
            int count = end - index - 1;
            if (count == 0) return;
            if (count <= IncrementalLimit)
            {
                for (int i = end - 1; i > index; i--) Rows.RemoveAt(i);
            }
            else
            {
                var keep = _selected.Where(r => !r.Node.IsDescendantOf(row.Node)).ToList();
                Rows.RemoveRange(index + 1, count);
                SelectionHost?.SelectRows(keep);
            }
        }
    }

    /// <summary>Recomputes filter visibility over the whole tree and regenerates the visible row list.</summary>
    private void RebuildRows(bool preserveSelection = true)
    {
        var selectedPaths = preserveSelection
            ? new HashSet<string>(_selected.Select(r => r.Node.RelativePath), StringComparer.OrdinalIgnoreCase)
            : new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        _visible.Clear();
        var list = new List<RowViewModel>();
        if (_result is not null)
        {
            ComputeVisible(_result.Root);
            foreach (var child in _result.Root.Children)
                if (_visible[child]) Emit(child, 0, list);
        }

        if (list.Count == Rows.Count && list.SequenceEqual(Rows))
        {
            foreach (var row in list) row.Refresh();
        }
        else
        {
            Rows.ReplaceAll(list);
            if (selectedPaths.Count > 0)
                SelectionHost?.SelectRows(list.Where(r => selectedPaths.Contains(r.Node.RelativePath)).ToList());
        }
    }

    private void EvictRows(DiffNode node)
    {
        _rowCache.Remove(node);
        foreach (var n in node.Descendants()) _rowCache.Remove(n);
    }

    private bool ComputeVisible(DiffNode node)
    {
        bool anyChild = false;
        foreach (var child in node.Children)
            anyChild |= ComputeVisible(child);

        bool visible = node.IsDirectory && node.Children.Count > 0
            ? anyChild || (SearchActive && NameMatches(node) && StatusPasses(node))
            : StatusPasses(node) && NameMatches(node);
        _visible[node] = visible;
        return visible;
    }

    private bool StatusPasses(DiffNode node) => node.Status switch
    {
        DiffStatus.OnlyLeft => ShowOnlyLeft,
        DiffStatus.OnlyRight => ShowOnlyRight,
        DiffStatus.NewerLeft => ShowNewerLeft,
        DiffStatus.NewerRight => ShowNewerRight,
        DiffStatus.Identical => ShowIdentical && !DifferencesOnly,
        DiffStatus.Differs => true,
        _ => ShowErrors,
    };

    private bool NameMatches(DiffNode node) =>
        !SearchActive || node.Name.Contains(SearchText.Trim(), StringComparison.OrdinalIgnoreCase);

    private void Emit(DiffNode node, int level, List<RowViewModel> list)
    {
        list.Add(GetRow(node, level));
        if (!node.IsDirectory || !(SearchActive || IsExpanded(node))) return;
        foreach (var child in node.Children)
            if (_visible.TryGetValue(child, out var v) && v) Emit(child, level + 1, list);
    }

    private RowViewModel GetRow(DiffNode node, int level)
    {
        if (_rowCache.TryGetValue(node, out var row))
        {
            row.Level = level;
            return row;
        }
        row = new RowViewModel(node, level, this);
        _rowCache[node] = row;
        return row;
    }

    [RelayCommand]
    private void ExpandAll()
    {
        if (_result is null) return;
        foreach (var n in _result.Root.Descendants())
            if (n.IsDirectory && n.Children.Count > 0) _expanded.Add(n.RelativePath);
        RebuildRows();
    }

    [RelayCommand]
    private void CollapseAll()
    {
        _expanded.Clear();
        RebuildRows();
    }

    [RelayCommand(CanExecute = nameof(HasSelection))]
    private void ExpandSubtree()
    {
        foreach (var row in _selected.ToList())
        {
            if (row.Node.IsDirectory) _expanded.Add(row.Node.RelativePath);
            foreach (var n in row.Node.Descendants())
                if (n.IsDirectory && n.Children.Count > 0) _expanded.Add(n.RelativePath);
        }
        RebuildRows();
    }

    [RelayCommand(CanExecute = nameof(HasSelection))]
    private void CollapseSubtree()
    {
        foreach (var row in _selected.ToList())
        {
            _expanded.Remove(row.Node.RelativePath);
            foreach (var n in row.Node.Descendants()) _expanded.Remove(n.RelativePath);
        }
        RebuildRows();
    }

    // ---- Sorting -----------------------------------------------------------------------------

    public void SortBy(SortKey key)
    {
        if (_sortKey == key) _sortAscending = !_sortAscending;
        else
        {
            _sortKey = key;
            _sortAscending = true;
        }
        ApplySort();
    }

    private void ApplySort()
    {
        if (_result is null) return;
        SortTree(_result.Root);
        RebuildRows();
    }

    private void SortTree(DiffNode node)
    {
        if (node.Children.Count == 0) return;
        node.Children.Sort(CompareNodes);
        foreach (var child in node.Children)
            if (child.IsDirectory) SortTree(child);
    }

    private int CompareNodes(DiffNode a, DiffNode b)
    {
        if (FoldersFirst && a.IsDirectory != b.IsDirectory) return a.IsDirectory ? -1 : 1;
        int c = _sortKey switch
        {
            SortKey.LeftSize => Size(a.Left, a.LeftTotalSize).CompareTo(Size(b.Left, b.LeftTotalSize)),
            SortKey.RightSize => Size(a.Right, a.RightTotalSize).CompareTo(Size(b.Right, b.RightTotalSize)),
            SortKey.LeftDate => (a.Left?.LastWriteUtc ?? DateTime.MinValue).CompareTo(b.Left?.LastWriteUtc ?? DateTime.MinValue),
            SortKey.RightDate => (a.Right?.LastWriteUtc ?? DateTime.MinValue).CompareTo(b.Right?.LastWriteUtc ?? DateTime.MinValue),
            SortKey.Status => a.Status.CompareTo(b.Status),
            _ => 0,
        };
        if (c == 0) c = string.Compare(a.Name, b.Name, StringComparison.OrdinalIgnoreCase);
        return _sortAscending ? c : -c;

        static long Size(FileInfoSnapshot? s, long total) => s is null ? -1 : total;
    }

    // ---- Selection ---------------------------------------------------------------------------

    public void UpdateSelection(IList added, IList removed)
    {
        foreach (RowViewModel r in removed) _selected.Remove(r);
        foreach (RowViewModel r in added) _selected.Add(r);
        UpdateSelectionText();
        RefreshCommandStates();
    }

    private bool HasSelection() => _selected.Count > 0;

    private List<DiffNode> SelectedNodes()
    {
        var set = _selected.Select(r => r.Node).ToHashSet();
        // Keep display order for predictable plans.
        return Rows.Where(r => set.Contains(r.Node)).Select(r => r.Node).ToList();
    }

    private void UpdateSelectionText()
    {
        if (_selected.Count == 0)
        {
            SelectionText = "";
            return;
        }
        var top = OperationPlanner.TopLevelOnly(_selected.Select(r => r.Node));
        long size = top.Sum(n => Math.Max(n.LeftTotalSize, n.RightTotalSize));
        SelectionText = $"{_selected.Count:N0} selected ({RowViewModel.FormatSize(size)})";
    }

    [RelayCommand]
    private void SelectByStatus(string status)
    {
        var wanted = Enum.Parse<DiffStatus>(status);
        SelectionHost?.SelectRows(Rows.Where(r => r.Node.Status == wanted).ToList());
    }

    [RelayCommand]
    private void SelectAll() => SelectionHost?.SelectRows(Rows.ToList());

    [RelayCommand]
    private void InvertSelection() => SelectionHost?.SelectRows(Rows.Where(r => !_selected.Contains(r)).ToList());

    [RelayCommand]
    private void FocusSearch() => SelectionHost?.FocusSearch();

    private void RefreshCommandStates()
    {
        CompareCommand.NotifyCanExecuteChanged();
        SwapCommand.NotifyCanExecuteChanged();
        CopyToRightCommand.NotifyCanExecuteChanged();
        CopyToLeftCommand.NotifyCanExecuteChanged();
        DeleteCommand.NotifyCanExecuteChanged();
        DeleteLeftCommand.NotifyCanExecuteChanged();
        DeleteRightCommand.NotifyCanExecuteChanged();
        DeleteBothCommand.NotifyCanExecuteChanged();
        RenameCommand.NotifyCanExecuteChanged();
        RefreshSelectedCommand.NotifyCanExecuteChanged();
        ExpandSubtreeCommand.NotifyCanExecuteChanged();
        CollapseSubtreeCommand.NotifyCanExecuteChanged();
        ExportReportCommand.NotifyCanExecuteChanged();
        CompareContentsCommand.NotifyCanExecuteChanged();
    }

    // ---- Counts ------------------------------------------------------------------------------

    private void UpdateCounts()
    {
        if (_result is null)
        {
            CountsText = "";
            return;
        }
        var counts = new Dictionary<DiffStatus, int>();
        foreach (var n in _result.Root.Descendants())
        {
            if (n.IsDirectory && n.Status is not (DiffStatus.Error or DiffStatus.TypeConflict)) continue;
            counts[n.Status] = counts.GetValueOrDefault(n.Status) + 1;
        }

        var parts = new List<string>();
        void Add(DiffStatus s, string label)
        {
            if (counts.TryGetValue(s, out var c) && c > 0) parts.Add($"{c:N0} {label}");
        }
        Add(DiffStatus.OnlyLeft, "only left");
        Add(DiffStatus.NewerLeft, "newer left");
        Add(DiffStatus.NewerRight, "newer right");
        Add(DiffStatus.OnlyRight, "only right");
        Add(DiffStatus.TypeConflict, "conflicts");
        Add(DiffStatus.Error, "errors");
        Add(DiffStatus.Identical, "identical");
        CountsText = parts.Count == 0 ? "No files" : string.Join(" · ", parts);
    }

    // ---- Copy / delete / rename ----------------------------------------------------------------

    private bool CanCopyToRight() => !IsBusy && _selected.Any(r => r.Node.ExistsLeft);
    private bool CanCopyToLeft() => !IsBusy && _selected.Any(r => r.Node.ExistsRight);
    private bool CanDeleteLeft() => !IsBusy && _selected.Any(r => r.Node.ExistsLeft);
    private bool CanDeleteRight() => !IsBusy && _selected.Any(r => r.Node.ExistsRight);
    private bool CanDeleteAny() => !IsBusy && _selected.Count > 0;
    private bool CanRename() => !IsBusy && _selected.Count == 1;
    private bool CanRefreshSelected() => !IsBusy && _selected.Count > 0 && _result is not null;
    private bool CanExport() => !IsBusy && _result is not null;

    [RelayCommand(CanExecute = nameof(CanCopyToRight))]
    private Task CopyToRightAsync() => CopyAsync(CopyDirection.LeftToRight);

    [RelayCommand(CanExecute = nameof(CanCopyToLeft))]
    private Task CopyToLeftAsync() => CopyAsync(CopyDirection.RightToLeft);

    [RelayCommand(CanExecute = nameof(CanDeleteAny))]
    private Task DeleteAsync()
    {
        var nodes = SelectedNodes();
        Sides sides = Sides.Both;
        if (nodes.Any(n => n.ExistsLeft && n.ExistsRight))
        {
            sides = _dialogs.AskDeleteSide();
            if (sides == Sides.None) return Task.CompletedTask;
        }
        return DeleteSidesAsync(nodes, sides);
    }

    [RelayCommand(CanExecute = nameof(CanDeleteLeft))]
    private Task DeleteLeftAsync() => DeleteSidesAsync(SelectedNodes(), Sides.Left);

    [RelayCommand(CanExecute = nameof(CanDeleteRight))]
    private Task DeleteRightAsync() => DeleteSidesAsync(SelectedNodes(), Sides.Right);

    [RelayCommand(CanExecute = nameof(CanDeleteAny))]
    private Task DeleteBothAsync() => DeleteSidesAsync(SelectedNodes(), Sides.Both);

    private async Task CopyAsync(CopyDirection direction)
    {
        if (_result is null) return;
        var nodes = SelectedNodes();
        var plan = OperationPlanner.PlanCopy(_result, nodes, direction);
        var title = direction == CopyDirection.LeftToRight ? "Copy to right" : "Copy to left";
        if (plan.Actions.Count == 0)
        {
            _dialogs.ShowInfo(title, "Nothing to copy." + (plan.Notes.Count > 0 ? "\n\n" + string.Join("\n", plan.Notes) : ""));
            return;
        }

        if (plan.Actions.Count >= Settings.PreviewThreshold &&
            !_dialogs.ShowPreview(new PreviewRequest(title, plan, IsDelete: false, Permanent: false, OfferDontAskAgain: false)).Confirmed)
            return;

        await ExecuteAsync(title, plan, nodes);
    }

    private async Task DeleteSidesAsync(List<DiffNode> nodes, Sides sides)
    {
        if (_result is null) return;
        var plan = OperationPlanner.PlanDelete(_result, nodes, sides);
        if (plan.Actions.Count == 0)
        {
            _dialogs.ShowInfo("Delete", "Nothing to delete on the chosen side.");
            return;
        }

        bool recycle = Settings.DeleteToRecycleBin;
        if (!recycle || Settings.ConfirmRecycleDelete)
        {
            var title = recycle ? "Move to Recycle Bin" : "Delete permanently";
            var answer = _dialogs.ShowPreview(new PreviewRequest(title, plan, IsDelete: true, Permanent: !recycle, OfferDontAskAgain: recycle));
            if (!answer.Confirmed) return;
            if (answer.DontAskAgain)
            {
                Settings.ConfirmRecycleDelete = false;
                _settingsService.Save();
            }
        }
        if (!recycle && !_dialogs.Confirm("Delete permanently",
                $"Permanently delete {plan.Actions.Count:N0} item(s) ({RowViewModel.FormatSize(plan.TotalBytes)})?\n\nThis cannot be undone.", warning: true))
            return;

        await ExecuteAsync("Delete", plan, nodes);
    }

    private async Task ExecuteAsync(string title, OperationPlan plan, List<DiffNode> selection)
    {
        if (_result is null) return;
        var result = _result;
        _cts = new CancellationTokenSource();
        IsOperating = true;
        ProgressIndeterminate = false;
        ProgressValue = 0;
        FileProgressValue = 0;
        var options = new OperationOptions
        {
            LeftRoot = result.LeftRoot,
            RightRoot = result.RightRoot,
            Tolerance = result.Options.Tolerance,
            OverwritePolicy = Settings.OverwritePolicy,
            DeleteToRecycleBin = Settings.DeleteToRecycleBin,
        };
        var progress = new Progress<OperationProgress>(p =>
        {
            ProgressValue = p.TotalBytes > 0 ? 100.0 * p.TotalBytesDone / p.TotalBytes : p.ItemsTotal > 0 ? 100.0 * p.ItemsDone / p.ItemsTotal : 0;
            FileProgressValue = p.FileBytesTotal > 0 ? 100.0 * p.FileBytesDone / p.FileBytesTotal : 0;
            StatusText = $"{title}: {Math.Min(p.ItemsDone + 1, p.ItemsTotal):N0}/{p.ItemsTotal:N0}  {p.CurrentItem}";
        });
        var dispatcher = Application.Current.Dispatcher;
        OverwriteResolver resolver = (conflict, _) => dispatcher.InvokeAsync(() => _dialogs.AskOverwrite(conflict)).Task;

        OperationSummary summary;
        try
        {
            summary = await Task.Run(() => _executor.ExecuteAsync(plan.Actions, options, resolver, progress, _cts.Token));

            // Refresh only the affected rows instead of rescanning everything.
            var top = OperationPlanner.TopLevelOnly(selection);
            var targets = top.Where(n => n.Parent is not null).Select(n => (n.Parent!, n.Name)).ToList();
            var topSet = top.ToHashSet();
            foreach (var item in summary.Items)
            {
                if (item.FinalTargetPath is null || !topSet.Contains(item.Action.Node) || item.Action.Node.Parent is null) continue;
                var finalName = Path.GetFileName(item.FinalTargetPath);
                if (!string.Equals(finalName, item.Action.Node.Name, StringComparison.OrdinalIgnoreCase))
                    targets.Add((item.Action.Node.Parent, finalName));
            }
            await RefreshNodesAsync(result, targets);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "{Title} failed", title);
            _dialogs.ShowError(title, ex.Message);
            return;
        }
        finally
        {
            IsOperating = false;
            ProgressValue = 0;
            FileProgressValue = 0;
        }

        StatusText = $"{title}: {summary.Succeeded:N0} succeeded, {summary.Skipped:N0} skipped, {summary.Failed:N0} failed{(summary.Cancelled ? " (cancelled)" : "")}.";
        _dialogs.ShowSummary(title, summary, plan.Notes);
    }

    private async Task RefreshNodesAsync(DiffResult result, IReadOnlyCollection<(DiffNode Parent, string Name)> targets)
    {
        var distinct = targets
            .DistinctBy(t => (t.Parent, t.Name.ToUpperInvariant()))
            .ToList();
        var data = await Task.Run(() => distinct.Select(t => _engine.PrepareRefresh(result, t.Parent, t.Name)).ToList());
        if (!ReferenceEquals(result, _result)) return; // a new comparison replaced the tree meanwhile

        foreach (var d in data)
        {
            var old = d.Parent.Children.FirstOrDefault(c => string.Equals(c.Name, d.Name, StringComparison.OrdinalIgnoreCase));
            if (old is not null) EvictRows(old);
            DiffEngine.ApplyRefresh(d);
            if (d.Fresh is not null) SortTree(d.Fresh);
            d.Parent.Children.Sort(CompareNodes);
        }
        RebuildRows();
        UpdateCounts();
        UpdateSelectionText();
    }

    [RelayCommand(CanExecute = nameof(CanRefreshSelected))]
    private async Task RefreshSelectedAsync()
    {
        if (_result is null) return;
        var top = OperationPlanner.TopLevelOnly(SelectedNodes()).Where(n => n.Parent is not null).ToList();
        IsScanning = true;
        try
        {
            await RefreshNodesAsync(_result, top.Select(n => (n.Parent!, n.Name)).ToList());
            StatusText = $"Rescanned {top.Count:N0} selected item(s).";
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Refresh failed");
            _dialogs.ShowError("Refresh", ex.Message);
        }
        finally
        {
            IsScanning = false;
        }
    }

    [RelayCommand(CanExecute = nameof(CanRename))]
    private async Task RenameAsync()
    {
        if (_result is null || _selected.Count != 1) return;
        var node = _selected.First().Node;
        if (node.Parent is null) return;
        var answer = _dialogs.AskRename(new RenameRequest(node.Name, node.ExistsLeft, node.ExistsRight));
        if (answer is null || string.Equals(answer.NewName, node.Name, StringComparison.Ordinal)) return;

        var options = new OperationOptions { LeftRoot = _result.LeftRoot, RightRoot = _result.RightRoot };
        var errors = new List<string>();
        foreach (var (side, wanted) in new[] { (Side.Left, answer.Left), (Side.Right, answer.Right) })
        {
            if (!wanted || node.Get(side) is null) continue;
            try
            {
                _executor.Rename(options, side, node.RelativePath, answer.NewName);
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or ArgumentException or UnsafePathException)
            {
                errors.Add($"{side}: {ex.Message}");
            }
        }

        await RefreshNodesAsync(_result, [(node.Parent, node.Name), (node.Parent, answer.NewName)]);
        if (errors.Count > 0) _dialogs.ShowError("Rename", string.Join("\n", errors));
    }

    // ---- Text compare -----------------------------------------------------------------------

    private bool CanCompareContents() =>
        !IsBusy && _result is not null && _selected.Count == 1 &&
        _selected.First().Node is { IsDirectory: false } node && (node.ExistsLeft || node.ExistsRight);

    [RelayCommand(CanExecute = nameof(CanCompareContents))]
    private void CompareContents()
    {
        if (!CanCompareContents()) return;
        var result = _result!;
        var node = _selected.First().Node;
        _textCompare.Open(new TextCompareRequest(
            Path.Join(result.RootFor(Side.Left), node.RelativePath), node.ExistsLeft,
            Path.Join(result.RootFor(Side.Right), node.RelativePath), node.ExistsRight,
            node.RelativePath,
            () => _ = RescanAfterTextSaveAsync(result, node.RelativePath)));
    }

    /// <summary>Refreshes a file's row after the text compare window saved it.</summary>
    private async Task RescanAfterTextSaveAsync(DiffResult result, string relativePath)
    {
        if (!ReferenceEquals(result, _result) || IsBusy) return;
        var node = result.Root.Descendants().FirstOrDefault(n =>
            string.Equals(n.RelativePath, relativePath, StringComparison.OrdinalIgnoreCase));
        if (node?.Parent is null) return;
        try
        {
            await RefreshNodesAsync(result, new[] { (node.Parent, node.Name) });
        }
        catch (Exception ex)
        {
            _logger.LogWarning(ex, "Could not rescan {Path} after saving", relativePath);
        }
    }

    public bool ConfirmCloseTextCompares() => _textCompare.ConfirmCloseAll();

    // ---- Shell integration ------------------------------------------------------------------

    private string? PathOf(string side)
    {
        if (_result is null || _selected.Count == 0) return null;
        var node = _selected.First().Node;
        var s = side == "Right" ? Side.Right : Side.Left;
        if (node.Get(s) is null) return null;
        return Path.Join(_result.RootFor(s), node.RelativePath);
    }

    [RelayCommand]
    private void Open(string side) => Shell(side, p => _shell.Open(p));

    [RelayCommand]
    private void OpenContainingFolder(string side) => Shell(side, p => _shell.Open(Path.GetDirectoryName(p)!));

    [RelayCommand]
    private void ShowInExplorer(string side) => Shell(side, p => _shell.ShowInExplorer(p));

    private void Shell(string side, Action<string> action)
    {
        var path = PathOf(side);
        if (path is null) return;
        try
        {
            action(path);
        }
        catch (Exception ex)
        {
            _dialogs.ShowError("Open", ex.Message);
        }
    }

    [RelayCommand]
    private void CopyPath(string which)
    {
        if (_result is null || _selected.Count == 0) return;
        var lines = new List<string>();
        foreach (var node in SelectedNodes())
        {
            if (which is "Left" or "Both" && node.ExistsLeft) lines.Add(Path.Join(_result.LeftRoot, node.RelativePath));
            if (which is "Right" or "Both" && node.ExistsRight) lines.Add(Path.Join(_result.RightRoot, node.RelativePath));
        }
        if (lines.Count == 0) return;
        try
        {
            _shell.SetClipboardText(string.Join(Environment.NewLine, lines));
        }
        catch (Exception ex)
        {
            _dialogs.ShowError("Copy path", ex.Message);
        }
    }

    /// <summary>Tools menu checkbox: the "Select as left" / "Compare to left" entries in the Explorer context menu.</summary>
    [ObservableProperty] private bool _explorerIntegration;

    partial void OnExplorerIntegrationChanged(bool value)
    {
        try
        {
            if (value) _explorer.Register();
            else _explorer.Unregister();
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Could not change the Explorer context menu integration");
            _dialogs.ShowError("Explorer context menu", ex.Message);
            Dispatcher.CurrentDispatcher.BeginInvoke(() => ExplorerIntegration = _explorer.IsRegistered);
        }
    }

    [RelayCommand]
    private void OpenLogFolder()
    {
        try
        {
            Directory.CreateDirectory(ShellService.LogDirectory);
            _shell.Open(ShellService.LogDirectory);
        }
        catch (Exception ex)
        {
            _dialogs.ShowError("Open log folder", ex.Message);
        }
    }

    // ---- Report ----------------------------------------------------------------------------

    [RelayCommand(CanExecute = nameof(CanExport))]
    private void ExportReport()
    {
        if (_result is null) return;
        var path = _dialogs.PickSaveFile("FolderCompare report.csv", "CSV file (*.csv)|*.csv|Text file (*.txt)|*.txt");
        if (path is null) return;

        // The current filter, independent of which folders are expanded.
        var nodes = _result.Root.Descendants().Where(n => _visible.GetValueOrDefault(n)).ToList();
        try
        {
            using var writer = new StreamWriter(path, false, new UTF8Encoding(true));
            if (path.EndsWith(".txt", StringComparison.OrdinalIgnoreCase)) ReportExporter.WriteText(writer, _result, nodes);
            else ReportExporter.WriteCsv(writer, _result, nodes);
            StatusText = $"Report with {nodes.Count:N0} rows written to {path}.";
        }
        catch (Exception ex)
        {
            _dialogs.ShowError("Export report", ex.Message);
        }
    }

    // ---- Settings & profiles -----------------------------------------------------------------

    [RelayCommand]
    private void OpenSettings()
    {
        SaveSettings();
        var edited = _dialogs.EditSettings(Settings);
        if (edited is null) return;
        _settingsService.Replace(edited);
        _suppressFilterRebuild = true;
        ExcludePatterns = edited.ExcludePatterns;
        IncludePatterns = edited.IncludePatterns;
        FoldersFirst = edited.FoldersFirst;
        _suppressFilterRebuild = false;
        if (_result is not null) StatusText = "Settings saved. Press F5 to rescan with the new options.";
    }

    [RelayCommand]
    private void SaveProfile()
    {
        var name = _dialogs.AskText("Save profile", "Profile name:", SelectedProfile?.Name ?? "");
        if (string.IsNullOrWhiteSpace(name)) return;
        var profile = new Profile
        {
            Name = name.Trim(),
            LeftPath = LeftPath,
            RightPath = RightPath,
            ToleranceSeconds = Settings.ToleranceSeconds,
            IgnoreDstOffset = Settings.IgnoreDstOffset,
            IgnoreHiddenAndSystem = Settings.IgnoreHiddenAndSystem,
            Recursive = Settings.Recursive,
            ExcludePatterns = ExcludePatterns,
            IncludePatterns = IncludePatterns,
            Filters = CurrentFilters(),
        };
        var existing = Profiles.FirstOrDefault(p => string.Equals(p.Name, profile.Name, StringComparison.OrdinalIgnoreCase));
        if (existing is not null)
        {
            if (!_dialogs.Confirm("Save profile", $"Replace the existing profile '{existing.Name}'?")) return;
            Profiles[Profiles.IndexOf(existing)] = profile;
        }
        else
        {
            Profiles.Add(profile);
        }
        _suppressProfileApply = true;
        SelectedProfile = profile;
        _suppressProfileApply = false;
        SaveSettings();
    }

    [RelayCommand]
    private void DeleteProfile()
    {
        if (SelectedProfile is null) return;
        if (!_dialogs.Confirm("Delete profile", $"Delete the profile '{SelectedProfile.Name}'?")) return;
        Profiles.Remove(SelectedProfile);
        SelectedProfile = null;
        SaveSettings();
    }

    private void ApplyProfile(Profile p)
    {
        _suppressFilterRebuild = true;
        LeftPath = p.LeftPath;
        RightPath = p.RightPath;
        Settings.ToleranceSeconds = p.ToleranceSeconds;
        Settings.IgnoreDstOffset = p.IgnoreDstOffset;
        Settings.IgnoreHiddenAndSystem = p.IgnoreHiddenAndSystem;
        Settings.Recursive = p.Recursive;
        ExcludePatterns = p.ExcludePatterns;
        IncludePatterns = p.IncludePatterns;
        ApplyFilters(p.Filters);
        _suppressFilterRebuild = false;
        RebuildRows();
        StatusText = $"Profile '{p.Name}' loaded. Press F5 to compare.";
    }
}
