using Avalonia;
using Avalonia.Media;
using AvaloniaEdit.Document;
using AvaloniaEdit.Rendering;
using FolderCompare.Core.TextDiff;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>Paints line backgrounds, in-line differences, filler rows and the current-section outline of one pane.</summary>
internal sealed class DiffBackgroundRenderer : IBackgroundRenderer
{
    private const double HatchSpacing = 8;

    private readonly FillerElementGenerator _fillers;
    private readonly bool _isLeft;

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
        double width = Math.Max(textView.Bounds.Width, 1);
        var changedLine = DiffColors.ChangedLine;
        var unimportantLine = DiffColors.UnimportantLine;

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
            dc.FillRectangle(important ? changedLine : unimportantLine, new Rect(0, textTop, width, lh));

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
                    dc.FillRectangle(textBrush, new Rect(r.X, textTop, r.Width, lh));
            }
        }

        if (CurrentBlock >= 0 && CurrentBlock < comparison.Blocks.Count)
        {
            var block = comparison.Blocks[CurrentBlock];
            double y = comparison.RowOf(CurrentBlock) * lh - textView.VerticalOffset;
            var pen = new Pen(DiffColors.CurrentSection, 1.5);
            dc.DrawRectangle(null, pen, new Rect(0.75, y + 0.75, width - 1.5, Math.Max(block.Height * lh - 1.5, 2)));
        }
    }

    /// <summary>A light background with diagonal hatching, so filler rows don't look like empty lines.</summary>
    private static void DrawFiller(DrawingContext dc, Rect rect)
    {
        dc.FillRectangle(DiffColors.Filler, rect);
        var pen = new Pen(DiffColors.FillerHatch, 1);
        using (dc.PushClip(rect))
        {
            // Lines from bottom-left to top-right, anchored to absolute coordinates so the pattern doesn't shift.
            double start = Math.Floor((rect.Left - rect.Height) / HatchSpacing) * HatchSpacing;
            for (double x = start; x < rect.Right; x += HatchSpacing)
                dc.DrawLine(pen, new Point(x, rect.Bottom), new Point(x + rect.Height, rect.Top));
        }
    }
}
