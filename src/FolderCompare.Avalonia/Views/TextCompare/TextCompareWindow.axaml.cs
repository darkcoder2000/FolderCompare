using System.IO.Abstractions;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Documents;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Media;
using Avalonia.Threading;
using AvaloniaEdit;
using AvaloniaEdit.Document;
using AvaloniaEdit.Rendering;
using FolderCompare.App.Services;
using FolderCompare.Core.TextDiff;
using Microsoft.Extensions.Logging;
using TextRange = FolderCompare.Core.TextDiff.TextRange;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>
/// Side-by-side text compare: two editable panes kept line-aligned with filler rows, recompared while typing,
/// with navigation between difference sections and copying of sections to the other side.
/// </summary>
public partial class TextCompareWindow : Window
{
    private readonly TextCompareLauncher _launcher = null!;
    private readonly IFileSystem _fs = null!;
    private readonly SettingsService _settings = null!;
    private readonly ILogger _logger = null!;
    private readonly TextCompareRequest _request = null!;
    private readonly Pane _left = null!, _right = null!;
    private readonly DispatcherTimer _recompareTimer = null!;
    /// <summary>Ctrl on Windows and Linux, Cmd on macOS.</summary>
    private readonly KeyModifiers _command;

    private TextComparison? _comparison;
    private Pane _focused = null!;
    private int _currentBlock = -1;
    private int _version;
    private bool _syncingScroll, _syncingCaret, _closeConfirmed;

    public TextCompareWindow() => InitializeComponent();

    internal TextCompareWindow(TextCompareLauncher launcher, IFileSystem fs, SettingsService settings, ILogger logger,
                               TextCompareRequest request, TextSide left, TextSide right) : this()
    {
        _launcher = launcher;
        _fs = fs;
        _settings = settings;
        _logger = logger;
        _request = request;
        _command = OperatingSystem.IsMacOS() ? KeyModifiers.Meta : KeyModifiers.Control;

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
        WindowPlacement.Restore(this, s.Window, 300, 200);
        ApplyShortcutTexts();

        foreach (var pane in new[] { _left, _right }) SetUpPane(pane);
        Load(_left, left);
        Load(_right, right);
        ApplyShowWhitespace();

        CenterStrip.CopyRequested += (block, toRight) => Copy(toRight, new[] { block });
        Overview.ScrollRequested += row => ScrollToRow(row, center: true);
        AddHandler(KeyDownEvent, Window_KeyDown, RoutingStrategies.Tunnel);
        Closing += Window_Closing;
        Opened += async (_, _) =>
        {
            WindowPlacement.FitToWorkArea(this);
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
    }

    private Pane Other(Pane p) => p.IsLeft ? _right : _left;

    private string Shortcut(string key) => (OperatingSystem.IsMacOS() ? "Cmd+" : "Ctrl+") + key;

    private void ApplyShortcutTexts()
    {
        SaveFocusedMenu.InputGesture = new KeyGesture(Key.S, _command);
        SaveAllMenu.InputGesture = new KeyGesture(Key.S, _command | KeyModifiers.Shift);
        UndoMenu.InputGesture = new KeyGesture(Key.Z, _command);
        RedoMenu.InputGesture = new KeyGesture(Key.Y, _command);
        CopyRightMenu.InputGesture = new KeyGesture(Key.R, _command);
        CopyLeftMenu.InputGesture = new KeyGesture(Key.L, _command);
        NextMenu.InputGesture = new KeyGesture(Key.N, _command);
        PrevMenu.InputGesture = new KeyGesture(Key.P, _command);
        ToolTip.SetTip(SaveButton, $"Save the focused side ({Shortcut("S")})");
        ToolTip.SetTip(UndoButton, $"Undo in the focused side ({Shortcut("Z")})");
        ToolTip.SetTip(RedoButton, $"Redo in the focused side ({Shortcut("Y")})");
        ToolTip.SetTip(PrevButton, $"Previous difference ({Shortcut("P")})");
        ToolTip.SetTip(NextButton, $"Next difference ({Shortcut("N")})");
        ToolTip.SetTip(CopyRightButton, $"Copy the current section to the right ({Shortcut("R")})");
        ToolTip.SetTip(CopyLeftButton, $"Copy the current section to the left ({Shortcut("L")})");
        CenterStrip.CopyRightShortcut = Shortcut("R");
        CenterStrip.CopyLeftShortcut = Shortcut("L");
    }

    // ---- Setup and loading ---------------------------------------------------------------------

    private void SetUpPane(Pane p)
    {
        var view = p.Editor.TextArea.TextView;
        view.ElementGenerators.Add(p.Fillers);
        view.BackgroundRenderers.Add(p.Renderer);
        p.Editor.Options.EnableHyperlinks = false;
        p.Editor.Options.EnableEmailHyperlinks = false;
        p.Editor.Options.AllowScrollBelowDocument = false;

        p.Editor.TextArea.Caret.PositionChanged += (_, _) => OnCaretMoved(p);
        p.Editor.TextArea.GotFocus += (_, _) =>
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
            p.Fillers.Side = comparison.Get(p.IsLeft);
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
        Dispatcher.UIThread.Post(UpdateMargins, DispatcherPriority.Render);
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
        _left.Editor.TextArea.TextView.InvalidateLayer(KnownLayer.Background);
        _right.Editor.TextArea.TextView.InvalidateLayer(KnownLayer.Background);

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
        double height = view.Bounds.Height;
        if (top < view.VerticalOffset || bottom > view.VerticalOffset + height)
            ScrollToRow(c.RowOf(block) - height / lh / 3, center: false);
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
        double offset = row * view.DefaultLineHeight - (center ? view.Bounds.Height / 2 : 0);
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
            // The two panes scroll together; the guard stops the other pane's echo from scrolling back.
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
            margin.ViewportHeight = view.Bounds.Height;
        }
    }

    // ---- Headers and line details ----------------------------------------------------------------

    private void UpdateHeaders()
    {
        foreach (var p in new[] { _left, _right })
        {
            p.Header.Text = (p.IsDirty ? "* " : "") + p.Path + (p.Exists ? "" : "   (does not exist; saving creates it)");
            ToolTip.SetTip(p.Header, p.Path);
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
        var inlines = block.Inlines ??= new InlineCollection();
        inlines.Clear();
        block.Background = null;
        if (line < 0 || line >= p.Document.LineCount) return;
        var docLine = p.Document.GetLineByNumber(line + 1);
        string text = p.Document.GetText(docLine.Offset, Math.Min(docLine.Length, 2000)).Replace('\t', ' ');
        var side = _comparison?.Get(p.IsLeft);
        var kind = side is not null && line < side.LineCount ? side.Kinds[line] : DiffBlockKind.Equal;
        if (kind != DiffBlockKind.Equal)
            block.Background = kind == DiffBlockKind.Unimportant ? DiffColors.UnimportantLine : DiffColors.ChangedLine;

        var ranges = side is not null && side.Inline.TryGetValue(line, out var r) ? r : Array.Empty<TextRange>();
        var highlight = kind == DiffBlockKind.Unimportant ? DiffColors.UnimportantText : DiffColors.ChangedText;
        int pos = 0;
        foreach (var range in ranges)
        {
            int start = Math.Min(range.Start, text.Length), end = Math.Min(range.End, text.Length);
            if (start > pos) inlines.Add(new Run(text[pos..start]));
            if (end > start) inlines.Add(new Run(text[start..end]) { Background = highlight });
            pos = Math.Max(pos, end);
        }
        if (pos < text.Length) inlines.Add(new Run(text[pos..]));
    }

    // ---- Saving, reloading, closing --------------------------------------------------------------

    private Task<MessageResult> AskAsync(string title, string message, MessageKind kind, MessageButtons buttons, MessageResult defaultResult) =>
        MessageDialog.Create(title, message, kind, buttons, defaultResult).ShowDialog<MessageResult>(this);

    private async Task<bool> SaveAsync(Pane p)
    {
        try
        {
            if (p.Exists && _fs.File.Exists(p.Path) && p.LoadedWriteUtc is { } loaded &&
                _fs.File.GetLastWriteTimeUtc(p.Path) != loaded &&
                await AskAsync("Save", $"{p.Path}\n\nhas been changed on disk since it was loaded. Overwrite it?",
                    MessageKind.Warning, MessageButtons.YesNo, MessageResult.No) != MessageResult.Yes)
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
            await AskAsync("Save", $"Could not save {p.Path}:\n\n{ex.Message}", MessageKind.Error, MessageButtons.Ok, MessageResult.Ok);
            return false;
        }
    }

    private async Task<bool> SaveAllAsync()
    {
        bool left = !_left.IsDirty || await SaveAsync(_left);
        bool right = !_right.IsDirty || await SaveAsync(_right);
        return left && right;
    }

    private async Task ReloadAsync()
    {
        if ((_left.IsDirty || _right.IsDirty) &&
            await AskAsync("Reload", "Discard your unsaved changes and reload both files from disk?",
                MessageKind.Warning, MessageButtons.YesNo, MessageResult.No) != MessageResult.Yes)
            return;

        var request = _request with
        {
            LeftExists = _fs.File.Exists(_left.Path),
            RightExists = _fs.File.Exists(_right.Path),
        };
        if (!_launcher.TryLoad(request, out var left, out var right, out var error))
        {
            await AskAsync("Reload", error, MessageKind.Info, MessageButtons.Ok, MessageResult.Ok);
            return;
        }
        Load(_left, left);
        Load(_right, right);
        RecompareNow();
    }

    /// <summary>Offers to save unsaved changes. False when the user cancelled.</summary>
    public async Task<bool> ConfirmCloseAsync()
    {
        if (!_left.IsDirty && !_right.IsDirty) return true;
        Activate();
        var sides = string.Join(" and ", new[] { _left, _right }.Where(p => p.IsDirty).Select(p => p.Path));
        var answer = await AskAsync(_request.Title, $"Save changes to\n{sides}?",
            MessageKind.Question, MessageButtons.YesNoCancel, MessageResult.Cancel);
        return answer switch
        {
            MessageResult.Yes => await SaveAllAsync(),
            MessageResult.No => true,
            _ => false,
        };
    }

    public void CloseWithoutPrompt()
    {
        _closeConfirmed = true;
        Close();
    }

    private async void Window_Closing(object? sender, WindowClosingEventArgs e)
    {
        if (!_closeConfirmed)
        {
            // The question is asynchronous: cancel this close and close again once it is answered.
            e.Cancel = true;
            if (!await ConfirmCloseAsync()) return;
            _closeConfirmed = true;
            SaveViewSettings();
            Close();
            return;
        }
        SaveViewSettings();
    }

    private bool _viewSettingsSaved;

    private void SaveViewSettings()
    {
        if (_viewSettingsSaved) return;
        _viewSettingsSaved = true;
        var s = _settings.Current.TextCompare;
        s.IgnoreWhitespace = IgnoreWhitespaceToggle.IsChecked == true;
        s.IgnoreCase = IgnoreCaseToggle.IsChecked == true;
        s.ShowWhitespace = ShowWhitespaceMenu.IsChecked;
        s.Window = WindowPlacement.Save(this, s.Window);
        _settings.Save();
    }

    // ---- Options -----------------------------------------------------------------------------------

    private async void Options_Changed(object? sender, RoutedEventArgs e)
    {
        // Keep the menu items and the toolbar toggles in step.
        if (sender == IgnoreWhitespaceMenu) IgnoreWhitespaceToggle.IsChecked = IgnoreWhitespaceMenu.IsChecked;
        else if (sender == IgnoreWhitespaceToggle) IgnoreWhitespaceMenu.IsChecked = IgnoreWhitespaceToggle.IsChecked == true;
        else if (sender == IgnoreCaseMenu) IgnoreCaseToggle.IsChecked = IgnoreCaseMenu.IsChecked;
        else if (sender == IgnoreCaseToggle) IgnoreCaseMenu.IsChecked = IgnoreCaseToggle.IsChecked == true;
        await RecompareAsync();
    }

    private void ShowWhitespace_Changed(object? sender, RoutedEventArgs e) => ApplyShowWhitespace();

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

    private void Window_KeyDown(object? sender, KeyEventArgs e)
    {
        var mods = e.KeyModifiers;
        var cmd = _command;
        Action? action = e.Key switch
        {
            Key.S when mods == cmd => () => _ = SaveAsync(_focused),
            Key.S when mods == (cmd | KeyModifiers.Shift) => () => _ = SaveAllAsync(),
            Key.F5 when mods == KeyModifiers.None => () => _ = ReloadAsync(),
            Key.N when mods == cmd => () => GoTo(NextDifference(Anchor())),
            Key.Down when mods == KeyModifiers.Alt => () => GoTo(NextDifference(Anchor())),
            Key.P when mods == cmd => () => GoTo(PreviousDifference(Anchor())),
            Key.Up when mods == KeyModifiers.Alt => () => GoTo(PreviousDifference(Anchor())),
            Key.R when mods == cmd => () => Copy(toRight: true),
            Key.Right when mods == KeyModifiers.Alt => () => Copy(toRight: true),
            Key.L when mods == cmd => () => Copy(toRight: false),
            Key.Left when mods == KeyModifiers.Alt => () => Copy(toRight: false),
            Key.Escape when mods == KeyModifiers.None => Close,
            _ => null,
        };
        if (action is null) return;
        action();
        e.Handled = true;
    }

    private async void SaveFocused_Click(object? sender, RoutedEventArgs e) => await SaveAsync(_focused);
    private async void SaveLeft_Click(object? sender, RoutedEventArgs e) => await SaveAsync(_left);
    private async void SaveRight_Click(object? sender, RoutedEventArgs e) => await SaveAsync(_right);
    private async void SaveAll_Click(object? sender, RoutedEventArgs e) => await SaveAllAsync();
    private async void Reload_Click(object? sender, RoutedEventArgs e) => await ReloadAsync();
    private void Close_Click(object? sender, RoutedEventArgs e) => Close();
    private void Undo_Click(object? sender, RoutedEventArgs e) => _focused.Editor.Undo();
    private void Redo_Click(object? sender, RoutedEventArgs e) => _focused.Editor.Redo();
    private void CopyToRight_Click(object? sender, RoutedEventArgs e) => Copy(toRight: true);
    private void CopyToLeft_Click(object? sender, RoutedEventArgs e) => Copy(toRight: false);
    private void NextDiff_Click(object? sender, RoutedEventArgs e) => GoTo(NextDifference(Anchor()));
    private void PrevDiff_Click(object? sender, RoutedEventArgs e) => GoTo(PreviousDifference(Anchor()));
    private void FirstDiff_Click(object? sender, RoutedEventArgs e) => GoTo(NextDifference(-1));
    private void LastDiff_Click(object? sender, RoutedEventArgs e) => GoTo(PreviousDifference(-1));
}
