using System.Text;

namespace FolderCompare.Core.TextDiff;

public sealed record TextDiffOptions
{
    /// <summary>Treat lines that differ only in the amount of whitespace as unimportant.</summary>
    public bool IgnoreWhitespace { get; init; }

    /// <summary>Treat lines that differ only in letter case as unimportant.</summary>
    public bool IgnoreCase { get; init; }
}

public enum DiffBlockKind
{
    Equal,
    /// <summary>Lines differ only in ways the options declare unimportant (shown blue).</summary>
    Unimportant,
    Changed,
    LeftOnly,
    RightOnly,
}

/// <summary>A run of lines. For one-sided blocks the empty side's start is the insertion point.</summary>
public sealed record DiffBlock(int LeftStart, int LeftCount, int RightStart, int RightCount, DiffBlockKind Kind)
{
    public int LeftEnd => LeftStart + LeftCount;
    public int RightEnd => RightStart + RightCount;
    public bool IsDifference => Kind is DiffBlockKind.Changed or DiffBlockKind.LeftOnly or DiffBlockKind.RightOnly;
    public int Height => Math.Max(LeftCount, RightCount);
}

public static class LineDiffer
{
    /// <summary>Compares two line lists and returns consecutive blocks that together cover both sides.</summary>
    public static IReadOnlyList<DiffBlock> Compare(IReadOnlyList<string> left, IReadOnlyList<string> right, TextDiffOptions options)
    {
        var keys = new Dictionary<string, int>(StringComparer.Ordinal);
        int[] a = Keys(left, options, keys);
        int[] b = Keys(right, options, keys);
        var (modA, modB) = MyersDiff.Compute(a, b);

        var blocks = new List<DiffBlock>();
        int i = 0, j = 0;
        while (i < a.Length || j < b.Length)
        {
            int si = i, sj = j;
            while (i < a.Length && j < b.Length && !modA[i] && !modB[j]) { i++; j++; }
            if (i > si) AddEqualRun(blocks, left, right, si, sj, i - si);

            si = i; sj = j;
            while (i < a.Length && modA[i]) i++;
            while (j < b.Length && modB[j]) j++;
            int ca = i - si, cb = j - sj;
            if (ca == 0 && cb == 0) continue;
            var kind = ca == 0 ? DiffBlockKind.RightOnly : cb == 0 ? DiffBlockKind.LeftOnly : DiffBlockKind.Changed;
            blocks.Add(new DiffBlock(si, ca, sj, cb, kind));
        }
        return blocks;
    }

    /// <summary>Splits an equal-key run into Equal and Unimportant blocks by comparing the raw text.</summary>
    private static void AddEqualRun(List<DiffBlock> blocks, IReadOnlyList<string> left, IReadOnlyList<string> right, int i, int j, int count)
    {
        int start = 0;
        while (start < count)
        {
            bool same = string.Equals(left[i + start], right[j + start], StringComparison.Ordinal);
            int end = start + 1;
            while (end < count && string.Equals(left[i + end], right[j + end], StringComparison.Ordinal) == same) end++;
            blocks.Add(new DiffBlock(i + start, end - start, j + start, end - start, same ? DiffBlockKind.Equal : DiffBlockKind.Unimportant));
            start = end;
        }
    }

    private static int[] Keys(IReadOnlyList<string> lines, TextDiffOptions options, Dictionary<string, int> keys)
    {
        var result = new int[lines.Count];
        for (int n = 0; n < lines.Count; n++)
        {
            var key = Normalize(lines[n], options);
            if (!keys.TryGetValue(key, out var id))
            {
                id = keys.Count;
                keys.Add(key, id);
            }
            result[n] = id;
        }
        return result;
    }

    internal static string Normalize(string line, TextDiffOptions options)
    {
        if (options.IgnoreWhitespace)
        {
            var sb = new StringBuilder(line.Length);
            bool pendingSpace = false;
            foreach (var c in line)
            {
                if (char.IsWhiteSpace(c))
                {
                    pendingSpace = sb.Length > 0;
                    continue;
                }
                if (pendingSpace) sb.Append(' ');
                pendingSpace = false;
                sb.Append(c);
            }
            line = sb.ToString();
        }
        return options.IgnoreCase ? line.ToUpperInvariant() : line;
    }
}

public static class TextLines
{
    /// <summary>Splits text at \r\n, \n or \r. Like an editor, "a\n" yields two lines ("a" and "").</summary>
    public static List<string> Split(string text)
    {
        var lines = new List<string>();
        int start = 0;
        for (int n = 0; n < text.Length; n++)
        {
            char c = text[n];
            if (c != '\r' && c != '\n') continue;
            lines.Add(text.Substring(start, n - start));
            if (c == '\r' && n + 1 < text.Length && text[n + 1] == '\n') n++;
            start = n + 1;
        }
        lines.Add(text.Substring(start));
        return lines;
    }
}
