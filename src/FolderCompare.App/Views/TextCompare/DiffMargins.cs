using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using FolderCompare.Core.TextDiff;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>Shared layout state of the two aligned panes, pushed in by the window.</summary>
internal abstract class DiffMarginBase : FrameworkElement
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

    protected static Brush BandBrush(DiffBlock b) =>
        b.Kind == DiffBlockKind.Unimportant ? DiffColors.UnimportantText : DiffColors.ChangedText;
}

/// <summary>The strip between the panes: one band per difference section with copy arrows.</summary>
internal sealed class DiffCenterStrip : DiffMarginBase
{
    private static readonly Brush Background = Frozen(new SolidColorBrush(Color.FromRgb(0xF4, 0xF4, 0xF4)));

    /// <summary>Raised with the block index and true for "copy to right".</summary>
    public event Action<int, bool>? CopyRequested;

    public DiffCenterStrip()
    {
        Width = 34;
        Cursor = Cursors.Arrow;
    }

    protected override void OnRender(DrawingContext dc)
    {
        double w = ActualWidth, h = ActualHeight;
        dc.DrawRectangle(Background, null, new Rect(0, 0, w, h));
        var c = Comparison;
        if (c is null) return;

        for (int n = 0; n < c.Blocks.Count; n++)
        {
            var b = c.Blocks[n];
            if (!IsShown(b)) continue;
            var (top, height) = Span(c, n);
            if (top > h || top + height < 0) continue;
            dc.DrawRectangle(BandBrush(b), null, new Rect(3, top, w - 6, height));
            if (n == CurrentBlock)
            {
                var pen = new Pen(DiffColors.CurrentSection, 1.5);
                pen.Freeze();
                dc.DrawRectangle(null, pen, new Rect(3, top, w - 6, height));
            }
            double cy = top + Math.Min(LineHeight, height) / 2;
            DrawArrow(dc, w * 0.30, cy, right: true);
            DrawArrow(dc, w * 0.70, cy, right: false);
        }
    }

    private (double Top, double Height) Span(TextComparison c, int block) =>
        (c.RowOf(block) * LineHeight - VerticalOffset, Math.Max(c.Blocks[block].Height * LineHeight, 4));

    private static void DrawArrow(DrawingContext dc, double cx, double cy, bool right)
    {
        double s = 4.5, d = right ? 1 : -1;
        var g = new StreamGeometry();
        using (var ctx = g.Open())
        {
            ctx.BeginFigure(new Point(cx - d * s, cy - s), true, true);
            ctx.LineTo(new Point(cx + d * s, cy), true, false);
            ctx.LineTo(new Point(cx - d * s, cy + s), true, false);
        }
        g.Freeze();
        dc.DrawGeometry(DiffColors.Glyph, null, g);
    }

    private (int Block, bool ToRight)? HitTest(Point p)
    {
        var c = Comparison;
        if (c is null) return null;
        for (int n = 0; n < c.Blocks.Count; n++)
        {
            if (!IsShown(c.Blocks[n])) continue;
            var (top, height) = Span(c, n);
            if (p.Y >= top && p.Y <= top + Math.Max(height, LineHeight)) return (n, p.X < ActualWidth / 2);
        }
        return null;
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        var hit = HitTest(e.GetPosition(this));
        Cursor = hit is null ? Cursors.Arrow : Cursors.Hand;
        ToolTip = hit is null ? null : hit.Value.ToRight ? "Copy this section to the right (Ctrl+R)" : "Copy this section to the left (Ctrl+L)";
    }

    protected override void OnMouseLeftButtonDown(MouseButtonEventArgs e)
    {
        if (HitTest(e.GetPosition(this)) is { } hit)
        {
            CopyRequested?.Invoke(hit.Block, hit.ToRight);
            e.Handled = true;
        }
    }

    protected override HitTestResult HitTestCore(PointHitTestParameters p) => new PointHitTestResult(this, p.HitPoint);

    private static Brush Frozen(Brush b)
    {
        b.Freeze();
        return b;
    }
}

/// <summary>A thumbnail of the whole comparison: one mark per difference and the visible area.</summary>
internal sealed class DiffOverviewBar : DiffMarginBase
{
    private static readonly Brush Background = Frozen(new SolidColorBrush(Color.FromRgb(0xFA, 0xFA, 0xFA)));
    private static readonly Pen ViewportPen = FrozenPen(new Pen(new SolidColorBrush(Color.FromArgb(0xB0, 0x40, 0x40, 0x40)), 1));
    private static readonly Brush ViewportFill = Frozen(new SolidColorBrush(Color.FromArgb(0x22, 0x40, 0x40, 0x40)));

    /// <summary>Raised with the aligned row that should be centred in the panes.</summary>
    public event Action<double>? ScrollRequested;

    public DiffOverviewBar()
    {
        Width = 14;
        Cursor = Cursors.Hand;
        ToolTip = "Overview: click to jump";
    }

    private double Scale(TextComparison c) => ActualHeight / Math.Max(c.TotalRows, 1);

    protected override void OnRender(DrawingContext dc)
    {
        double w = ActualWidth;
        dc.DrawRectangle(Background, null, new Rect(0, 0, w, ActualHeight));
        var c = Comparison;
        if (c is null) return;
        double scale = Scale(c);
        for (int n = 0; n < c.Blocks.Count; n++)
        {
            var b = c.Blocks[n];
            if (!IsShown(b)) continue;
            dc.DrawRectangle(BandBrush(b), null, new Rect(2, c.RowOf(n) * scale, w - 4, Math.Max(b.Height * scale, 2)));
        }
        if (LineHeight > 0)
        {
            double top = VerticalOffset / LineHeight * scale;
            double height = Math.Max(ViewportHeight / LineHeight * scale, 4);
            dc.DrawRectangle(ViewportFill, ViewportPen, new Rect(0.5, top + 0.5, w - 1, Math.Min(height, ActualHeight - top) - 1));
        }
    }

    protected override void OnMouseLeftButtonDown(MouseButtonEventArgs e)
    {
        CaptureMouse();
        Jump(e.GetPosition(this));
        e.Handled = true;
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        if (IsMouseCaptured) Jump(e.GetPosition(this));
    }

    protected override void OnMouseLeftButtonUp(MouseButtonEventArgs e) => ReleaseMouseCapture();

    private void Jump(Point p)
    {
        if (Comparison is { } c) ScrollRequested?.Invoke(p.Y / Scale(c));
    }

    protected override HitTestResult HitTestCore(PointHitTestParameters p) => new PointHitTestResult(this, p.HitPoint);

    private static Brush Frozen(Brush b)
    {
        b.Freeze();
        return b;
    }

    private static Pen FrozenPen(Pen p)
    {
        p.Freeze();
        return p;
    }
}
