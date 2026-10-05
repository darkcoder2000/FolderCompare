namespace FolderCompare.Core.TextDiff;

public readonly record struct TextRange(int Start, int Length)
{
    public int End => Start + Length;
}

/// <summary>Finds the differing parts inside a pair of changed lines, at word granularity.</summary>
public static class InlineDiffer
{
    public static (IReadOnlyList<TextRange> Left, IReadOnlyList<TextRange> Right) Compare(string left, string right, TextDiffOptions? options = null)
    {
        options ??= new TextDiffOptions();
        var lt = Tokenize(left);
        var rt = Tokenize(right);
        var keys = new Dictionary<string, int>(StringComparer.Ordinal);
        var a = lt.Select(t => Key(left, t, options, keys)).ToArray();
        var b = rt.Select(t => Key(right, t, options, keys)).ToArray();
        var (modA, modB) = MyersDiff.Compute(a, b);
        return (Ranges(lt, modA, left, options), Ranges(rt, modB, right, options));
    }

    private static int Key(string s, TextRange t, TextDiffOptions options, Dictionary<string, int> keys)
    {
        var text = s.Substring(t.Start, t.Length);
        if (options.IgnoreWhitespace && char.IsWhiteSpace(text[0])) text = " ";
        if (options.IgnoreCase) text = text.ToUpperInvariant();
        if (!keys.TryGetValue(text, out var id))
        {
            id = keys.Count;
            keys.Add(text, id);
        }
        return id;
    }

    /// <summary>Merges adjacent modified tokens into ranges. Whitespace-only ranges are dropped when whitespace is ignored.</summary>
    private static List<TextRange> Ranges(List<TextRange> tokens, bool[] modified, string s, TextDiffOptions options)
    {
        var result = new List<TextRange>();
        for (int n = 0; n < tokens.Count; n++)
        {
            if (!modified[n]) continue;
            if (options.IgnoreWhitespace && char.IsWhiteSpace(s[tokens[n].Start])) continue;
            var t = tokens[n];
            if (result.Count > 0 && result[^1].End == t.Start)
                result[^1] = new TextRange(result[^1].Start, result[^1].Length + t.Length);
            else
                result.Add(t);
        }
        return result;
    }

    /// <summary>Tokens are runs of letters/digits/underscore, runs of whitespace, or single other characters.</summary>
    internal static List<TextRange> Tokenize(string s)
    {
        var tokens = new List<TextRange>();
        int n = 0;
        while (n < s.Length)
        {
            int start = n;
            char c = s[n];
            if (IsWord(c)) while (n < s.Length && IsWord(s[n])) n++;
            else if (char.IsWhiteSpace(c)) while (n < s.Length && char.IsWhiteSpace(s[n])) n++;
            else n++;
            tokens.Add(new TextRange(start, n - start));
        }
        return tokens;
    }

    private static bool IsWord(char c) => char.IsLetterOrDigit(c) || c == '_';
}
