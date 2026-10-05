using System.Windows;
using System.Windows.Media.TextFormatting;
using FolderCompare.Core.TextDiff;
using ICSharpCode.AvalonEdit.Rendering;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>
/// Adds empty filler rows above or below document lines so that corresponding lines of both panes sit at
/// the same height. Each affected line gets one zero-length element at its end whose reported ascent/descent
/// makes the visual line taller; the filler area itself is painted by <see cref="DiffBackgroundRenderer"/>.
/// </summary>
internal sealed class FillerElementGenerator : VisualLineElementGenerator
{
    public TextComparison.Side? Side { get; set; }

    public (int Above, int Below) FillerFor(int lineIndex)
    {
        var side = Side;
        if (side is null || lineIndex >= side.LineCount) return (0, 0);
        int above = lineIndex == 0 ? side.FillerAbove : 0;
        int below = side.FillerBelow.GetValueOrDefault(lineIndex);
        return (above, below);
    }

    public override int GetFirstInterestedOffset(int startOffset)
    {
        var line = CurrentContext.VisualLine.LastDocumentLine;
        var (above, below) = FillerFor(line.LineNumber - 1);
        if (above + below == 0) return -1;
        return startOffset <= line.EndOffset ? line.EndOffset : -1;
    }

    public override VisualLineElement ConstructElement(int offset)
    {
        var line = CurrentContext.VisualLine.LastDocumentLine;
        var (above, below) = FillerFor(line.LineNumber - 1);
        var view = CurrentContext.TextView;
        return new FillerElement(above, below, view.DefaultLineHeight, view.DefaultBaseline);
    }

    private sealed class FillerElement : VisualLineElement
    {
        private readonly int _above, _below;
        private readonly double _lineHeight, _baseline;

        public FillerElement(int above, int below, double lineHeight, double baseline) : base(1, 0)
        {
            _above = above;
            _below = below;
            _lineHeight = lineHeight;
            _baseline = baseline;
        }

        public override TextRun CreateTextRun(int startVisualColumn, ITextRunConstructionContext context) =>
            new FillerRun(TextRunProperties, (_above + _below + 1) * _lineHeight, _above * _lineHeight + _baseline);

        // The caret never stops on the filler.
        public override int GetNextCaretPosition(int visualColumn, System.Windows.Documents.LogicalDirection direction, ICSharpCode.AvalonEdit.Document.CaretPositioningMode mode) => -1;
    }

    private sealed class FillerRun : TextEmbeddedObject
    {
        private readonly double _height, _baseline;

        public FillerRun(TextRunProperties properties, double height, double baseline)
        {
            Properties = properties;
            _height = height;
            _baseline = baseline;
        }

        public override TextRunProperties Properties { get; }
        public override int Length => 1;
        public override CharacterBufferReference CharacterBufferReference => default;
        public override LineBreakCondition BreakBefore => LineBreakCondition.BreakPossible;
        public override LineBreakCondition BreakAfter => LineBreakCondition.BreakPossible;
        public override bool HasFixedSize => true;

        public override TextEmbeddedObjectMetrics Format(double remainingParagraphWidth) => new(0, _height, _baseline);
        public override Rect ComputeBoundingBox(bool rightToLeft, bool sideways) => new(0, -_baseline, 0, _height);
        public override void Draw(System.Windows.Media.DrawingContext drawingContext, Point origin, bool rightToLeft, bool sideways) { }
    }
}
