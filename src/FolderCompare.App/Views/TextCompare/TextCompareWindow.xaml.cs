using System.ComponentModel;
using System.IO;
using System.IO.Abstractions;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using TextRange = FolderCompare.Core.TextDiff.TextRange;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;
using FolderCompare.App.Services;
using FolderCompare.Core.TextDiff;
using ICSharpCode.AvalonEdit;
using ICSharpCode.AvalonEdit.Document;
using Microsoft.Extensions.Logging;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>
/// Side-by-side text compare: two editable panes kept line-aligned with filler rows, recompared while typing,
/// with navigation between difference sections and copying of sections to the other side.
/// </summary>
public partial class TextCompareWindow : Window
{
    private readonly TextCompareLauncher _launcher;
    private readonly IFileSystem _fs;
    private readonly SettingsService _settings;
    private readonly ILogger _logger;
    private readonly TextCompareRequest _request;
    private readonly Pane _left, _right;
    private readonly DispatcherTimer _recompareTimer;

    private TextComparison? _comparison;
    private Pane _focused;
    private int _currentBlock = -1;
    private int _version;
    private bool _syncingScroll, _syncingCaret, _closeWithoutPrompt;

    internal TextCompareWindow(TextCompareLauncher launcher, IFileSystem fs, SettingsService settings, ILogger logger,
                               TextCompareRequest request, TextSide left, TextSide right)
    {
        InitializeComponent();
        _launcher = launcher;
        _fs = fs;
        _settings = settings;
        _logger = logger;
        _request = request;
        Icon = Application.Current.MainWindow?.Icon;

        _left = new Pane(true, LeftEditor, LeftHeader, LeftFormat);
        _right = new Pane(false, RightEditor, RightHeader, RightFormat);
        _focused = _left;

        _recompareTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(250) };
        _recompareTimer.Tick += async (_, _) => await RecompareAsync();

        var s = settings.Current.TextCompare;
        IgnoreWhitespaceMenu.IsChecked = s.IgnoreWhitespace;
        IgnoreWhitespaceToggle.IsChecked = s.IgnoreWhitespace;
        IgnoreCaseMenu.IsChecked = s.IgnoreCase;
        IgnoreCaseToggle.IsChecked = s.IgnoreCase;
        ShowWhitespaceMenu.IsChecked = s.ShowWhitespace;
        ApplySavedBounds(s.Window);

        foreach (var pane in new[] { _left, _right }) SetUpPane(pane);
        Load(_left, left);
        Load(_right, right);
        ApplyShowWhitespace();

        CenterStrip.CopyRequested += (block, toRight) => Copy(toRight, new[] { block });
        Overview.ScrollRequested += row => ScrollToRow(row, center: true);
        Loaded += async (_, _) =>
        {
            await RecompareAsync();
            if (_comparison is { DifferenceCount: > 0 }) GoTo(NextDifference(-1));
            _left.Editor.TextArea.Focus();
        };
    }

    private sealed class Pane
    {
        public Pane(bool isLeft, TextEditor editor, TextBlock header, TextBlock format)
        {
            IsLeft = isLeft;
            Editor = editor;
            Header = header;
            FormatText = format;
            Renderer = new DiffBackgroundRenderer(Fillers, isLeft);
        }

        public bool IsLeft { get; }
        public TextEditor Editor { get; }
        public TextBlock Header { get; }
        public TextBlock FormatText { get; }
        public FillerElementGenerator Fillers { get; } = new();
        public DiffBackgroundRenderer Renderer { get; }
        public string Path { get; set; } = "";
        public bool Exists { get; set; }
        public LoadedText Format { get; set; } = LoadedText.Empty;
        public DateTime? LoadedWriteUtc { get; set; }

        public TextDocument Document => Editor.Document;
        public bool IsDirty => !Document.UndoStack.IsOriginalFile;
        public int CaretLine => Editor.TextArea.Caret.Line - 1;
        public string Name => IsLeft ? "left" : "right";
    }

    private Pane Other(Pane p) => p.IsLeft ? _right : _left;

    // ---- Setup and loading ---------------------------------------------------------------------

    private void SetUpPane(Pane p)
    {
        var view = p.Editor.TextArea.TextView;
        view.ElementGenerators.Add(p.Fillers);
        view.BackgroundRenderers.Add(p.Renderer);
        p.Editor.Options.EnableHyperlinks = false;
        p.Editor.Options.EnableEmailHyperlinks = false;
        p.Editor.Options.AllowScrollBelowDocument = false;
        // Both panes always show a horizontal scroll bar so their viewports keep the same height.
        p.Editor.HorizontalScrollBarVisibility = ScrollBarVisibility.Visible;

        p.Editor.TextArea.Caret.PositionChanged += (_, _) => OnCaretMoved(p);
        p.Editor.TextArea.GotKeyboardFocus += (_, _) =>
        {
            _focused = p;
            OnCaretMoved(p);
        };
        view.ScrollOffsetChanged += (_, _) => OnScrolled(p);
        view.SizeChanged += (_, _) => UpdateMargins();
    }

    private void Load(Pane p, TextSide side)
    {
        p.Path = side.Path;
        p.Exists = side.Exists;
        p.Format = side.Text;
        p.LoadedWriteUtc = side.LastWriteUtc;

        var document = new TextDocument(side.Text.Text);
        document.TextChanged += (_, _) => ScheduleRecompare();
        document.UndoStack.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(UndoStack.IsOriginalFile)) UpdateHeaders();
        };
        p.Editor.Document = document;
        UpdateHeaders();
    }

    private void ApplySavedBounds(WindowSettings? w)
    {
        if (w is null || w.Width < 300 || w.Height < 200) return;
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

    // ---- Comparison ------------------------------------------------------------------------------

    private TextDiffOptions Options() => new()
    {
        IgnoreWhitespace = IgnoreWhitespaceToggle.IsChecked == true,
        IgnoreCase = IgnoreCaseToggle.IsChecked == true,
    };

    private void ScheduleRecompare()
    {
        _version++;
        _recompareTimer.Stop();
        _recompareTimer.Start();
    }

    private async Task RecompareAsync()
    {
        _recompareTimer.Stop();
        int version = ++_version;
        string left = _left.Document.Text, right = _right.Document.Text;
        var options = Options();
        try
        {
            var comparison = await Task.Run(() => TextComparison.Compute(left, right, options));
            if (version == _version) Apply(comparison);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Text comparison failed");
            DiffCountText.Text = "Comparison failed: " + ex.Message;
        }
    }

    /// <summary>Recompares synchronously, e.g. right before or after copying a section.</summary>
    private void RecompareNow()
    {
        _recompareTimer.Stop();
        _version++;
        Apply(TextComparison.Compute(_left.Document.Text, _right.Document.Text, Options()));
    }

    private void Apply(TextComparison comparison)
    {
        _comparison = comparison;
        foreach (var p in new[] { _left, _right })
        {
            var side = comparison.Get(p.IsLeft);
            p.Fillers.Side = side;
            p.Renderer.Comparison = comparison;
            // Rebuild the visual lines: filler heights change the layout.
            p.Editor.TextArea.TextView.Redraw();
        }
        Overview.Comparison = comparison;
        CenterStrip.Comparison = comparison;
        SetCurrentBlock(BlockForCaret(_focused));

        DiffCountText.Text = comparison.DifferenceCount switch
        {
            0 when comparison.Blocks.Any(b => b.Kind == DiffBlockKind.Unimportant) => "Only unimportant differences",
            0 => "No differences",
            1 => "1 difference section",
            var n => $"{n:N0} difference sections",
        };
        Dispatcher.BeginInvoke(UpdateMargins, DispatcherPriority.Render);
    }

    // ---- Current section and navigation --------------------------------------------------------

    /// <summary>The non-equal block at the caret, including a block that has no lines on this side but is inserted right there.</summary>
    private int BlockForCaret(Pane p)
    {
        var c = _comparison;
        if (c is null) return -1;
        int line = p.CaretLine;
        int n = c.BlockAt(p.IsLeft, line);
        if (n >= 0 && c.Blocks[n].Kind != DiffBlockKind.Equal) return n;

        int previous = n >= 0 ? n - 1 : c.Blocks.Count - 1;
        if (previous >= 0)
        {
            var b = c.Blocks[previous];
            int count = p.IsLeft ? b.LeftCount : b.RightCount;
            int start = p.IsLeft ? b.LeftStart : b.RightStart;
            if (b.Kind != DiffBlockKind.Equal && count == 0 && (start == line || (n < 0 && start == line + 1)))
                return previous;
        }
        return -1;
    }

    private void SetCurrentBlock(int block)
    {
        _currentBlock = block;
        _left.Renderer.CurrentBlock = _right.Renderer.CurrentBlock = block;
        CenterStrip.CurrentBlock = block;
        _left.Editor.TextArea.TextView.InvalidateLayer(ICSharpCode.AvalonEdit.Rendering.KnownLayer.Background);
        _right.Editor.TextArea.TextView.InvalidateLayer(ICSharpCode.AvalonEdit.Rendering.KnownLayer.Background);

        var c = _comparison;
        if (c is not null && block >= 0 && c.Blocks[block].IsDifference)
        {
            int index = c.Blocks.Take(block + 1).Count(b => b.IsDifference);
            SectionText.Text = $"Section {index:N0} of {c.DifferenceCount:N0}";
        }
        else
        {
            SectionText.Text = "";
        }
        PrevButton.IsEnabled = PreviousDifference(Anchor()) >= 0;
        NextButton.IsEnabled = NextDifference(Anchor()) >= 0;
    }

    /// <summary>The block navigation starts from: the current section, else the block at the caret.</summary>
    private int Anchor()
    {
        if (_currentBlock >= 0 || _comparison is null) return _currentBlock;
        int n = _comparison.BlockAt(_focused.IsLeft, _focused.CaretLine);
        return n >= 0 ? n : -1;
    }

    private int NextDifference(int from)
    {
        var c = _comparison;
        if (c is null) return -1;
        for (int n = from + 1; n < c.Blocks.Count; n++)
            if (c.Blocks[n].IsDifference) return n;
        return -1;
    }

    private int PreviousDifference(int from)
    {
        var c = _comparison;
        if (c is null) return -1;
        if (from < 0) from = c.Blocks.Count;
        for (int n = from - 1; n >= 0; n--)
            if (c.Blocks[n].IsDifference) return n;
        return -1;
    }

    private void GoTo(int block)
    {
        var c = _comparison;
        if (c is null || block < 0) return;
        var b = c.Blocks[block];
        _syncingCaret = true;
        try
        {
            SetCaretLine(_left, b.LeftStart);
            SetCaretLine(_right, b.RightStart);
        }
        finally
        {
            _syncingCaret = false;
        }
        SetCurrentBlock(block);
        UpdateDetails();

        // Scroll so that the section sits in the upper third, unless it is already fully visible.
        var view = _left.Editor.TextArea.TextView;
        double lh = view.DefaultLineHeight, top = c.RowOf(block) * lh, bottom = top + b.Height * lh;
        if (top < view.VerticalOffset || bottom > view.VerticalOffset + view.ActualHeight)
            ScrollToRow(c.RowOf(block) - view.ActualHeight / lh / 3, center: false);
    }

    private static void SetCaretLine(Pane p, int line)
    {
        line = Math.Clamp(line, 0, p.Document.LineCount - 1);
        p.Editor.TextArea.ClearSelection();
        p.Editor.TextArea.Caret.Position = new TextViewPosition(line + 1, 1);
    }

    private void ScrollToRow(double row, bool center)
    {
        var view = _left.Editor.TextArea.TextView;
        double offset = row * view.DefaultLineHeight - (center ? view.ActualHeight / 2 : 0);
        _left.Editor.ScrollToVerticalOffset(Math.Max(0, offset));
    }

    // ---- Copying sections ----------------------------------------------------------------------

    /// <summary>The sections the copy commands act on: those touched by a multi-line selection, else the one at the caret.</summary>
    private List<int> SelectedBlocks()
    {
        var c = _comparison;
        var p = _focused;
        var result = new List<int>();
        if (c is null) return result;

        var selection = p.Editor.TextArea.Selection;
        if (!selection.IsEmpty)
        {
            var segment = selection.SurroundingSegment;
            int first = p.Document.GetLineByOffset(segment.Offset).LineNumber - 1;
            var lastLine = p.Document.GetLineByOffset(segment.EndOffset);
            int last = lastLine.LineNumber - 1;
            if (segment.EndOffset == lastLine.Offset && last > first) last--;
            if (last > first)
            {
                for (int n = 0; n < c.Blocks.Count; n++)
                {
                    var b = c.Blocks[n];
                    if (b.Kind == DiffBlockKind.Equal) continue;
                    int start = p.IsLeft ? b.LeftStart : b.RightStart;
                    int count = p.IsLeft ? b.LeftCount : b.RightCount;
                    bool touches = count > 0 ? start <= last && start + count - 1 >= first : start > first && start <= last;
                    if (touches) result.Add(n);
                }
                return result;
            }
        }

        if (_currentBlock >= 0) result.Add(_currentBlock);
        return result;
    }

    private void Copy(bool toRight, IReadOnlyList<int>? blocks = null)
    {
        if (_recompareTimer.IsEnabled) RecompareNow();
        var c = _comparison;
        if (c is null) return;
        blocks ??= SelectedBlocks();
        if (blocks.Count == 0)
        {
            SectionText.Text = "Place the cursor in a difference section first.";
            return;
        }

        var source = toRight ? _left : _right;
        var target = Other(source);
        var sourceLines = new DocumentLineSource(source.Document);
        var targetLines = new DocumentLineSource(target.Document);
        // Edits are computed against the unchanged target and applied bottom-up, so offsets stay valid.
        var edits = blocks.OrderByDescending(n => n)
            .Select(n => BlockCopy.Compute(sourceLines, targetLines, c.Blocks[n], sourceIsLeft: toRight, target.Format.NewLine))
            .ToList();

        var first = c.Blocks[blocks.Min()];
        target.Document.BeginUpdate();   // one undo step for the whole copy
        try
        {
            foreach (var e in edits) target.Document.Replace(e.Offset, e.Length, e.Text);
        }
        finally
        {
            target.Document.EndUpdate();
        }
        RecompareNow();

        // Stay where the copy happened (its start lines are unchanged), so Ctrl+N continues from here.
        _syncingCaret = true;
        try
        {
            SetCaretLine(_left, first.LeftStart);
            SetCaretLine(_right, first.RightStart);
        }
        finally
        {
            _syncingCaret = false;
        }
        SetCurrentBlock(BlockForCaret(_focused));
        UpdateDetails();
    }

    // ---- Caret, scrolling, margins -----------------------------------------------------------

    private void OnCaretMoved(Pane p)
    {
        if (_syncingCaret || p != _focused) return;

        // Move the other caret to the matching line, without scrolling.
        if (_comparison is { } c)
        {
            var other = Other(p);
            int line = p.CaretLine;
            int? partner = c.PartnerLine(p.IsLeft, line);
            if (partner is null)
            {
                int n = c.BlockAt(p.IsLeft, line);
                if (n >= 0) partner = p.IsLeft ? c.Blocks[n].RightStart : c.Blocks[n].LeftStart;
            }
            if (partner is int target && target < other.Document.LineCount)
            {
                _syncingCaret = true;
                try
                {
                    other.Editor.TextArea.Caret.Position = new TextViewPosition(target + 1, 1);
                }
                finally
                {
                    _syncingCaret = false;
                }
            }
        }

        SetCurrentBlock(BlockForCaret(p));
        UpdateDetails();
    }

    private void OnScrolled(Pane p)
    {
        if (!_syncingScroll)
        {
            _syncingScroll = true;
            try
            {
                var offset = p.Editor.TextArea.TextView.ScrollOffset;
                var other = Other(p).Editor;
                if (Math.Abs(other.VerticalOffset - offset.Y) > 0.5) other.ScrollToVerticalOffset(offset.Y);
                if (Math.Abs(other.HorizontalOffset - offset.X) > 0.5) other.ScrollToHorizontalOffset(offset.X);
            }
            finally
            {
                _syncingScroll = false;
            }
        }
        UpdateMargins();
    }

    private void UpdateMargins()
    {
        var view = _left.Editor.TextArea.TextView;
        foreach (var margin in new DiffMarginBase[] { Overview, CenterStrip })
        {
            margin.LineHeight = view.DefaultLineHeight;
            margin.VerticalOffset = view.VerticalOffset;
            margin.ViewportHeight = view.ActualHeight;
        }
    }

    // ---- Headers and line details ----------------------------------------------------------------

    private void UpdateHeaders()
    {
        foreach (var p in new[] { _left, _right })
        {
            p.Header.Text = (p.IsDirty ? "* " : "") + p.Path + (p.Exists ? "" : "   (does not exist; saving creates it)");
            p.Header.ToolTip = p.Path;
            p.FormatText.Text = $"{p.Format.EncodingName} · {p.Format.NewLineName}";
        }
        bool dirty = _left.IsDirty || _right.IsDirty;
        Title = $"{(dirty ? "* " : "")}{_request.Title} - Text compare";
        SaveLeftMenu.IsEnabled = _left.IsDirty || !_left.Exists;
        SaveRightMenu.IsEnabled = _right.IsDirty || !_right.Exists;
    }

    private void UpdateDetails()
    {
        var caret = _focused.Editor.TextArea.Caret;
        CaretText.Text = $"{(_focused.IsLeft ? "Left" : "Right")}  Ln {caret.Line:N0}, Col {caret.Column:N0}";
        var c = _comparison;
        int leftLine = _left.CaretLine, rightLine = _right.CaretLine;
        if (c is not null && _focused is { } p)
        {
            int? partner = c.PartnerLine(p.IsLeft, p.CaretLine);
            if (p.IsLeft) rightLine = partner ?? -1;
            else leftLine = partner ?? -1;
        }
        FillDetails(LeftDetails, _left, leftLine);
        FillDetails(RightDetails, _right, rightLine);
    }

    private void FillDetails(TextBlock block, Pane p, int line)
    {
        block.Inlines.Clear();
        if (line < 0 || line >= p.Document.LineCount) return;
        var docLine = p.Document.GetLineByNumber(line + 1);
        string text = p.Document.GetText(docLine.Offset, Math.Min(docLine.Length, 2000)).Replace('\t', ' ');
        var side = _comparison?.Get(p.IsLeft);
        var kind = side is not null && line < side.LineCount ? side.Kinds[line] : DiffBlockKind.Equal;
        if (kind != DiffBlockKind.Equal)
            block.Background = kind == DiffBlockKind.Unimportant ? DiffColors.UnimportantLine : DiffColors.ChangedLine;
        else
            block.Background = Brushes.Transparent;

        var ranges = side is not null && side.Inline.TryGetValue(line, out var r) ? r : Array.Empty<TextRange>();
        var highlight = kind == DiffBlockKind.Unimportant ? DiffColors.UnimportantText : DiffColors.ChangedText;
        int pos = 0;
        foreach (var range in ranges)
        {
            int start = Math.Min(range.Start, text.Length), end = Math.Min(range.End, text.Length);
            if (start > pos) block.Inlines.Add(new Run(text[pos..start]));
            if (end > start) block.Inlines.Add(new Run(text[start..end]) { Background = highlight });
            pos = Math.Max(pos, end);
        }
        if (pos < text.Length) block.Inlines.Add(new Run(text[pos..]));
    }

    // ---- Saving, reloading, closing --------------------------------------------------------------

    private bool Save(Pane p)
    {
        try
        {
            if (p.Exists && _fs.File.Exists(p.Path) && p.LoadedWriteUtc is { } loaded &&
                _fs.File.GetLastWriteTimeUtc(p.Path) != loaded &&
                MessageBox.Show(this, $"{p.Path}\n\nhas been changed on disk since it was loaded. Overwrite it?",
                    "Save", MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No) != MessageBoxResult.Yes)
                return false;

            var directory = Path.GetDirectoryName(p.Path);
            if (!string.IsNullOrEmpty(directory)) _fs.Directory.CreateDirectory(directory);
            TextFileCodec.Save(_fs, p.Path, p.Document.Text, p.Format);
            _logger.LogInformation("Saved {Path} from text compare", p.Path);

            p.Exists = true;
            p.LoadedWriteUtc = _fs.File.GetLastWriteTimeUtc(p.Path);
            p.Document.UndoStack.MarkAsOriginalFile();
            UpdateHeaders();
            _request.FileSaved?.Invoke();
            return true;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            _logger.LogError(ex, "Could not save {Path}", p.Path);
            MessageBox.Show(this, $"Could not save {p.Path}:\n\n{ex.Message}", "Save", MessageBoxButton.OK, MessageBoxImage.Error);
            return false;
        }
    }

    private bool SaveAll() => (!_left.IsDirty || Save(_left)) & (!_right.IsDirty || Save(_right));

    private void Reload()
    {
        if ((_left.IsDirty || _right.IsDirty) &&
            MessageBox.Show(this, "Discard your unsaved changes and reload both files from disk?", "Reload",
                MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No) != MessageBoxResult.Yes)
            return;

        var request = _request with
        {
            LeftExists = _fs.File.Exists(_left.Path),
            RightExists = _fs.File.Exists(_right.Path),
        };
        if (!_launcher.TryLoad(request, out var left, out var right, out var error))
        {
            MessageBox.Show(this, error, "Reload", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }
        Load(_left, left);
        Load(_right, right);
        RecompareNow();
    }

    /// <summary>Offers to save unsaved changes. False when the user cancelled.</summary>
    public bool ConfirmClose()
    {
        if (!_left.IsDirty && !_right.IsDirty) return true;
        Activate();
        var sides = string.Join(" and ", new[] { _left, _right }.Where(p => p.IsDirty).Select(p => p.Path));
        var answer = MessageBox.Show(this, $"Save changes to\n{sides}?", _request.Title,
            MessageBoxButton.YesNoCancel, MessageBoxImage.Question, MessageBoxResult.Cancel);
        return answer switch
        {
            MessageBoxResult.Yes => SaveAll(),
            MessageBoxResult.No => true,
            _ => false,
        };
    }

    public void CloseWithoutPrompt()
    {
        _closeWithoutPrompt = true;
        Close();
    }

    private void Window_Closing(object? sender, CancelEventArgs e)
    {
        if (!_closeWithoutPrompt && !ConfirmClose())
        {
            e.Cancel = true;
            return;
        }

        var bounds = WindowState == WindowState.Normal ? new Rect(Left, Top, Width, Height) : RestoreBounds;
        var s = _settings.Current.TextCompare;
        s.IgnoreWhitespace = IgnoreWhitespaceToggle.IsChecked == true;
        s.IgnoreCase = IgnoreCaseToggle.IsChecked == true;
        s.ShowWhitespace = ShowWhitespaceMenu.IsChecked;
        s.Window = new WindowSettings
        {
            Left = bounds.Left,
            Top = bounds.Top,
            Width = bounds.Width,
            Height = bounds.Height,
            Maximized = WindowState == WindowState.Maximized,
        };
        _settings.Save();
    }

    // ---- Options -----------------------------------------------------------------------------------

    private async void Options_Changed(object sender, RoutedEventArgs e)
    {
        // Keep the menu items and the toolbar toggles in step.
        if (sender == IgnoreWhitespaceMenu) IgnoreWhitespaceToggle.IsChecked = IgnoreWhitespaceMenu.IsChecked;
        else if (sender == IgnoreWhitespaceToggle) IgnoreWhitespaceMenu.IsChecked = IgnoreWhitespaceToggle.IsChecked == true;
        else if (sender == IgnoreCaseMenu) IgnoreCaseToggle.IsChecked = IgnoreCaseMenu.IsChecked;
        else if (sender == IgnoreCaseToggle) IgnoreCaseMenu.IsChecked = IgnoreCaseToggle.IsChecked == true;
        await RecompareAsync();
    }

    private void ShowWhitespace_Changed(object sender, RoutedEventArgs e) => ApplyShowWhitespace();

    private void ApplyShowWhitespace()
    {
        bool show = ShowWhitespaceMenu.IsChecked;
        foreach (var p in new[] { _left, _right })
        {
            p.Editor.Options.ShowSpaces = show;
            p.Editor.Options.ShowTabs = show;
        }
    }

    // ---- Commands ----------------------------------------------------------------------------------

    private void Window_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        var mods = Keyboard.Modifiers;
        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        Action? action = (key, mods) switch
        {
            (Key.S, ModifierKeys.Control) => () => Save(_focused),
            (Key.S, ModifierKeys.Control | ModifierKeys.Shift) => () => SaveAll(),
            (Key.F5, ModifierKeys.None) => Reload,
            (Key.N, ModifierKeys.Control) or (Key.Down, ModifierKeys.Alt) => () => GoTo(NextDifference(Anchor())),
            (Key.P, ModifierKeys.Control) or (Key.Up, ModifierKeys.Alt) => () => GoTo(PreviousDifference(Anchor())),
            (Key.R, ModifierKeys.Control) or (Key.Right, ModifierKeys.Alt) => () => Copy(toRight: true),
            (Key.L, ModifierKeys.Control) or (Key.Left, ModifierKeys.Alt) => () => Copy(toRight: false),
            (Key.Escape, ModifierKeys.None) => Close,
            _ => null,
        };
        if (action is null) return;
        action();
        e.Handled = true;
    }

    private void SaveFocused_Click(object sender, RoutedEventArgs e) => Save(_focused);
    private void SaveLeft_Click(object sender, RoutedEventArgs e) => Save(_left);
    private void SaveRight_Click(object sender, RoutedEventArgs e) => Save(_right);
    private void SaveAll_Click(object sender, RoutedEventArgs e) => SaveAll();
    private void Reload_Click(object sender, RoutedEventArgs e) => Reload();
    private void Close_Click(object sender, RoutedEventArgs e) => Close();
    private void Undo_Click(object sender, RoutedEventArgs e) => _focused.Editor.Undo();
    private void Redo_Click(object sender, RoutedEventArgs e) => _focused.Editor.Redo();
    private void CopyToRight_Click(object sender, RoutedEventArgs e) => Copy(toRight: true);
    private void CopyToLeft_Click(object sender, RoutedEventArgs e) => Copy(toRight: false);
    private void NextDiff_Click(object sender, RoutedEventArgs e) => GoTo(NextDifference(Anchor()));
    private void PrevDiff_Click(object sender, RoutedEventArgs e) => GoTo(PreviousDifference(Anchor()));
    private void FirstDiff_Click(object sender, RoutedEventArgs e) => GoTo(NextDifference(-1));
    private void LastDiff_Click(object sender, RoutedEventArgs e) => GoTo(PreviousDifference(-1));
}
