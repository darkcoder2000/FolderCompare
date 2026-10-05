namespace FolderCompare.Core.TextDiff;

/// <summary>
/// The result of comparing two texts, prepared for a side-by-side view: per-line kinds, in-line ranges,
/// and the filler rows each side needs so that corresponding lines sit at the same height.
/// </summary>
public sealed class TextComparison
{
    /// <summary>In-line highlighting is skipped for lines longer than this; the whole line is marked instead.</summary>
    private const int MaxInlineLineLength = 4000;

    private readonly int[] _blockRow;

    private TextComparison(IReadOnlyList<DiffBlock> blocks, Side left, Side right)
    {
        Blocks = blocks;
        Left = left;
        Right = right;
        _blockRow = new int[blocks.Count];
        int row = 0;
        for (int n = 0; n < blocks.Count; n++)
        {
            _blockRow[n] = row;
            row += blocks[n].Height;
        }
        TotalRows = row;
        DifferenceCount = blocks.Count(b => b.IsDifference);
    }

    public IReadOnlyList<DiffBlock> Blocks { get; }
    public Side Left { get; }
    public Side Right { get; }
    public int DifferenceCount { get; }

    /// <summary>Number of aligned rows (lines plus fillers); the same on both sides.</summary>
    public int TotalRows { get; }

    public Side Get(bool left) => left ? Left : Right;

    /// <summary>The aligned row at which a block starts.</summary>
    public int RowOf(int blockIndex) => _blockRow[blockIndex];

    /// <summary>Index of the block that contains a line on one side, or -1. Blocks without lines on that side are never returned.</summary>
    public int BlockAt(bool left, int line)
    {
        int lo = 0, hi = Blocks.Count - 1;
        while (lo <= hi)
        {
            int mid = (lo + hi) / 2;
            var b = Blocks[mid];
            int start = left ? b.LeftStart : b.RightStart;
            int end = left ? b.LeftEnd : b.RightEnd;
            if (line < start) hi = mid - 1;
            else if (line >= end) lo = mid + 1;
            else return mid;
        }
        return -1;
    }

    /// <summary>The line on the other side that sits next to <paramref name="line"/>, if any.</summary>
    public int? PartnerLine(bool fromLeft, int line)
    {
        int n = BlockAt(fromLeft, line);
        if (n < 0) return null;
        var b = Blocks[n];
        int offset = line - (fromLeft ? b.LeftStart : b.RightStart);
        int otherCount = fromLeft ? b.RightCount : b.LeftCount;
        int otherStart = fromLeft ? b.RightStart : b.LeftStart;
        return offset < otherCount ? otherStart + offset : null;
    }

    public static TextComparison Compute(string leftText, string rightText, TextDiffOptions options)
    {
        var leftLines = TextLines.Split(leftText);
        var rightLines = TextLines.Split(rightText);
        var blocks = LineDiffer.Compare(leftLines, rightLines, options);
        var left = new Side(leftLines.Count);
        var right = new Side(rightLines.Count);
        var exact = new TextDiffOptions();

        foreach (var b in blocks)
        {
            for (int n = 0; n < b.LeftCount; n++) left.Kinds[b.LeftStart + n] = b.Kind;
            for (int n = 0; n < b.RightCount; n++) right.Kinds[b.RightStart + n] = b.Kind;

            if (b.Kind is DiffBlockKind.Changed or DiffBlockKind.Unimportant)
            {
                int pairs = Math.Min(b.LeftCount, b.RightCount);
                for (int n = 0; n < pairs; n++)
                {
                    var l = leftLines[b.LeftStart + n];
                    var r = rightLines[b.RightStart + n];
                    if (l.Length > MaxInlineLineLength || r.Length > MaxInlineLineLength) continue;
                    var (lr, rr) = InlineDiffer.Compare(l, r, b.Kind == DiffBlockKind.Unimportant ? exact : options);
                    if (lr.Count > 0) left.Inline[b.LeftStart + n] = lr;
                    if (rr.Count > 0) right.Inline[b.RightStart + n] = rr;
                }
            }

            AddFiller(left, b.LeftStart, b.LeftCount, b.Height - b.LeftCount);
            AddFiller(right, b.RightStart, b.RightCount, b.Height - b.RightCount);
        }
        return new TextComparison(blocks, left, right);
    }

    private static void AddFiller(Side side, int start, int count, int rows)
    {
        if (rows <= 0) return;
        if (count > 0 || start > 0)
        {
            int line = count > 0 ? start + count - 1 : start - 1;
            side.FillerBelow[line] = side.FillerBelow.GetValueOrDefault(line) + rows;
        }
        else
        {
            side.FillerAbove += rows;
        }
    }

    public sealed class Side
    {
        internal Side(int lineCount)
        {
            Kinds = new DiffBlockKind[lineCount];
        }

        public int LineCount => Kinds.Length;

        /// <summary>Kind of every line on this side.</summary>
        public DiffBlockKind[] Kinds { get; }

        /// <summary>Differing character ranges inside changed lines.</summary>
        public Dictionary<int, IReadOnlyList<TextRange>> Inline { get; } = new();

        /// <summary>Filler rows to show below a line.</summary>
        public Dictionary<int, int> FillerBelow { get; } = new();

        /// <summary>Filler rows to show above the first line.</summary>
        public int FillerAbove { get; set; }
    }
}
