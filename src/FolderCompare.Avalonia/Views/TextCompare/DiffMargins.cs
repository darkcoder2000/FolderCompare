using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Media;
using FolderCompare.Core.TextDiff;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>Shared layout state of the two aligned panes, pushed in by the window.</summary>
internal abstract class DiffMarginBase : Control
{
    private TextComparison? _comparison;
    private double _lineHeight = 16, _verticalOffset, _viewportHeight;
    private int _currentBlock = -1;

    public TextComparison? Comparison { get => _comparison; set { _comparison = value; InvalidateVisual(); } }
    public double LineHeight { get => _lineHeight; set { _lineHeight = value; InvalidateVisual(); } }
    public double VerticalOffset { get => _verticalOffset; set { _verticalOffset = value; InvalidateVisual(); } }
    public double ViewportHeight { get => _viewportHeight; set { _viewportHeight = value; InvalidateVisual(); } }
    public int CurrentBlock { get => _currentBlock; set { _currentBlock = value; InvalidateVisual(); } }

    protected static bool IsShown(DiffBlock b) => b.Kind != DiffBlockKind.Equal;

    protected static IBrush BandBrush(DiffBlock b) =>
        b.Kind == DiffBlockKind.Unimportant ? DiffColors.UnimportantText : DiffColors.ChangedText;
}

/// <summary>The strip between the panes: one band per difference section with copy arrows.</summary>
internal sealed class DiffCenterStrip : DiffMarginBase
{
    /// <summary>Raised with the block index and true for "copy to right".</summary>
    public event Action<int, bool>? CopyRequested;

    public DiffCenterStrip()
    {
        Width = 34;
        ClipToBounds = true;
    }

    /// <summary>Texts of the copy shortcuts for the tooltips, e.g. "Ctrl+R".</summary>
    public string CopyRightShortcut { get; set; } = "Ctrl+R";
    public string CopyLeftShortcut { get; set; } = "Ctrl+L";

    public override void Render(DrawingContext dc)
    {
        double w = Bounds.Width, h = Bounds.Height;
        dc.FillRectangle(DiffColors.CenterStrip, new Rect(0, 0, w, h));
        var c = Comparison;
        if (c is null) return;

        var glyph = DiffColors.Glyph;
        for (int n = 0; n < c.Blocks.Count; n++)
        {
            var b = c.Blocks[n];
            if (!IsShown(b)) continue;
            var (top, height) = Span(c, n);
            if (top > h || top + height < 0) continue;
            dc.FillRectangle(BandBrush(b), new Rect(3, top, w - 6, height));
            if (n == CurrentBlock)
                dc.DrawRectangle(null, new Pen(DiffColors.CurrentSection, 1.5), new Rect(3, top, w - 6, height));
            double cy = top + Math.Min(LineHeight, height) / 2;
            DrawArrow(dc, glyph, w * 0.30, cy, right: true);
            DrawArrow(dc, glyph, w * 0.70, cy, right: false);
        }
    }

    private (double Top, double Height) Span(TextComparison c, int block) =>
        (c.RowOf(block) * LineHeight - VerticalOffset, Math.Max(c.Blocks[block].Height * LineHeight, 4));

    private static void DrawArrow(DrawingContext dc, IBrush brush, double cx, double cy, bool right)
    {
        double s = 4.5, d = right ? 1 : -1;
        var g = new StreamGeometry();
        using (var ctx = g.Open())
        {
            ctx.BeginFigure(new Point(cx - d * s, cy - s), true);
            ctx.LineTo(new Point(cx + d * s, cy));
            ctx.LineTo(new Point(cx - d * s, cy + s));
            ctx.EndFigure(true);
        }
        dc.DrawGeometry(brush, null, g);
    }

    private (int Block, bool ToRight)? HitTest(Point p)
    {
        var c = Comparison;
        if (c is null) return null;
        for (int n = 0; n < c.Blocks.Count; n++)
        {
            if (!IsShown(c.Blocks[n])) continue;
            var (top, height) = Span(c, n);
            if (p.Y >= top && p.Y <= top + Math.Max(height, LineHeight)) return (n, p.X < Bounds.Width / 2);
        }
        return null;
    }

    protected override void OnPointerMoved(PointerEventArgs e)
    {
        base.OnPointerMoved(e);
        var hit = HitTest(e.GetPosition(this));
        Cursor = hit is null ? Cursor.Default : new Cursor(StandardCursorType.Hand);
        ToolTip.SetTip(this, hit is null ? null
            : hit.Value.ToRight ? $"Copy this section to the right ({CopyRightShortcut})"
            : $"Copy this section to the left ({CopyLeftShortcut})");
    }

    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        base.OnPointerPressed(e);
        if (!e.GetCurrentPoint(this).Properties.IsLeftButtonPressed) return;
        if (HitTest(e.GetPosition(this)) is { } hit)
        {
            CopyRequested?.Invoke(hit.Block, hit.ToRight);
            e.Handled = true;
        }
    }
}

/// <summary>A thumbnail of the whole comparison: one mark per difference and the visible area.</summary>
internal sealed class DiffOverviewBar : DiffMarginBase
{
    private bool _dragging;

    /// <summary>Raised with the aligned row that should be centred in the panes.</summary>
    public event Action<double>? ScrollRequested;

    public DiffOverviewBar()
    {
        Width = 14;
        ClipToBounds = true;
        Cursor = new Cursor(StandardCursorType.Hand);
        ToolTip.SetTip(this, "Overview: click to jump");
    }

    private double Scale(TextComparison c) => Bounds.Height / Math.Max(c.TotalRows, 1);

    public override void Render(DrawingContext dc)
    {
        double w = Bounds.Width, h = Bounds.Height;
        dc.FillRectangle(DiffColors.Overview, new Rect(0, 0, w, h));
        var c = Comparison;
        if (c is null) return;
        double scale = Scale(c);
        for (int n = 0; n < c.Blocks.Count; n++)
        {
            var b = c.Blocks[n];
            if (!IsShown(b)) continue;
            dc.FillRectangle(BandBrush(b), new Rect(2, c.RowOf(n) * scale, w - 4, Math.Max(b.Height * scale, 2)));
        }
        if (LineHeight > 0)
        {
            double top = VerticalOffset / LineHeight * scale;
            double height = Math.Max(ViewportHeight / LineHeight * scale, 4);
            dc.DrawRectangle(DiffColors.ViewportFill, new Pen(DiffColors.ViewportBorder, 1),
                             new Rect(0.5, top + 0.5, w - 1, Math.Max(Math.Min(height, h - top) - 1, 1)));
        }
    }

    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        base.OnPointerPressed(e);
        if (!e.GetCurrentPoint(this).Properties.IsLeftButtonPressed) return;
        _dragging = true;
        e.Pointer.Capture(this);
        Jump(e.GetPosition(this));
        e.Handled = true;
    }

    protected override void OnPointerMoved(PointerEventArgs e)
    {
        base.OnPointerMoved(e);
        if (_dragging) Jump(e.GetPosition(this));
    }

    protected override void OnPointerReleased(PointerReleasedEventArgs e)
    {
        base.OnPointerReleased(e);
        _dragging = false;
        e.Pointer.Capture(null);
    }

    private void Jump(Point p)
    {
        if (Comparison is { } c) ScrollRequested?.Invoke(p.Y / Scale(c));
    }
}
