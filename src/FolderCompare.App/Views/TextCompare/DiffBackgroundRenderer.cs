using System.Windows;
using System.Windows.Media;
using FolderCompare.Core.TextDiff;
using ICSharpCode.AvalonEdit.Document;
using ICSharpCode.AvalonEdit.Rendering;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>Paints line backgrounds, in-line differences, filler rows and the current-section outline of one pane.</summary>
internal sealed class DiffBackgroundRenderer : IBackgroundRenderer
{
    private readonly FillerElementGenerator _fillers;
    private readonly bool _isLeft;
    private static Brush? _hatch;

    public DiffBackgroundRenderer(FillerElementGenerator fillers, bool isLeft)
    {
        _fillers = fillers;
        _isLeft = isLeft;
    }

    public TextComparison? Comparison { get; set; }
    public int CurrentBlock { get; set; } = -1;

    public KnownLayer Layer => KnownLayer.Background;

    public void Draw(TextView textView, DrawingContext dc)
    {
        var comparison = Comparison;
        if (comparison is null || !textView.VisualLinesValid) return;
        var side = comparison.Get(_isLeft);
        double lh = textView.DefaultLineHeight;
        double width = Math.Max(textView.ActualWidth, 1);
        var document = textView.Document;

        foreach (var vl in textView.VisualLines)
        {
            var docLine = vl.FirstDocumentLine;
            int index = docLine.LineNumber - 1;
            var (above, below) = _fillers.FillerFor(index);
            double top = vl.VisualTop - textView.VerticalOffset;
            double textTop = top + above * lh;

            if (above > 0) DrawFiller(dc, new Rect(0, top, width, above * lh));
            if (below > 0) DrawFiller(dc, new Rect(0, textTop + lh, width, below * lh));
            if (index >= side.LineCount) continue;

            var kind = side.Kinds[index];
            if (kind == DiffBlockKind.Equal) continue;
            bool important = kind != DiffBlockKind.Unimportant;
            dc.DrawRectangle(important ? DiffColors.ChangedLine : DiffColors.UnimportantLine, null, new Rect(0, textTop, width, lh));

            if (!side.Inline.TryGetValue(index, out var ranges)) continue;
            var textBrush = important ? DiffColors.ChangedText : DiffColors.UnimportantText;
            foreach (var range in ranges)
            {
                // The comparison may lag behind typing for a moment; clamp to the current line.
                int start = Math.Min(range.Start, docLine.Length);
                int end = Math.Min(range.End, docLine.Length);
                if (end <= start) continue;
                var segment = new TextSegment { StartOffset = docLine.Offset + start, EndOffset = docLine.Offset + end };
                foreach (var r in BackgroundGeometryBuilder.GetRectsForSegment(textView, segment))
                    dc.DrawRectangle(textBrush, null, new Rect(r.X, textTop, r.Width, lh));
            }
        }

        if (CurrentBlock >= 0 && CurrentBlock < comparison.Blocks.Count)
        {
            var block = comparison.Blocks[CurrentBlock];
            double y = comparison.RowOf(CurrentBlock) * lh - textView.VerticalOffset;
            var pen = new Pen(DiffColors.CurrentSection, 1.5);
            pen.Freeze();
            dc.DrawRectangle(null, pen, new Rect(0.75, y + 0.75, width - 1.5, Math.Max(block.Height * lh - 1.5, 2)));
        }
    }

    private static void DrawFiller(DrawingContext dc, Rect rect)
    {
        dc.DrawRectangle(DiffColors.Filler, null, rect);
        dc.DrawRectangle(Hatch, null, rect);
    }

    private static Brush Hatch => _hatch ??= CreateHatch();

    private static Brush CreateHatch()
    {
        var pen = new Pen(DiffColors.FillerHatch, 1);
        var drawing = new GeometryDrawing(null, pen, new LineGeometry(new Point(0, 8), new Point(8, 0)));
        var brush = new DrawingBrush(drawing)
        {
            TileMode = TileMode.Tile,
            Viewport = new Rect(0, 0, 8, 8),
            ViewportUnits = BrushMappingMode.Absolute,
            Viewbox = new Rect(0, 0, 8, 8),
            ViewboxUnits = BrushMappingMode.Absolute,
        };
        brush.Freeze();
        return brush;
    }
}
