namespace FolderCompare.Core.TextDiff;

/// <summary>Line-oriented view of a text buffer (a string in tests, an editor document in the app).</summary>
public interface ILineSource
{
    int LineCount { get; }
    int TextLength { get; }
    /// <summary>Offset of the first character of a 0-based line.</summary>
    int LineStart(int line);
    /// <summary>Offset just after the last character of a line, excluding its line break.</summary>
    int LineEnd(int line);
    string GetText(int offset, int length);
}

public sealed class StringLineSource : ILineSource
{
    private readonly string _text;
    private readonly List<(int Start, int End)> _lines = new();

    public StringLineSource(string text)
    {
        _text = text;
        int start = 0;
        for (int n = 0; n < text.Length; n++)
        {
            char c = text[n];
            if (c != '\r' && c != '\n') continue;
            _lines.Add((start, n));
            if (c == '\r' && n + 1 < text.Length && text[n + 1] == '\n') n++;
            start = n + 1;
        }
        _lines.Add((start, text.Length));
    }

    public int LineCount => _lines.Count;
    public int TextLength => _text.Length;
    public int LineStart(int line) => _lines[line].Start;
    public int LineEnd(int line) => _lines[line].End;
    public string GetText(int offset, int length) => _text.Substring(offset, length);
}

/// <summary>A single replace operation on the target text.</summary>
public readonly record struct TextEdit(int Offset, int Length, string Text);

public static class BlockCopy
{
    /// <summary>
    /// Computes the edit that makes the target's lines of <paramref name="block"/> equal to the source's.
    /// <paramref name="sourceIsLeft"/> picks which side of the block is the source.
    /// Line breaks inside the copied text are converted to <paramref name="targetNewLine"/>.
    /// </summary>
    public static TextEdit Compute(ILineSource source, ILineSource target, DiffBlock block, bool sourceIsLeft, string targetNewLine)
    {
        int sStart = sourceIsLeft ? block.LeftStart : block.RightStart;
        int sCount = sourceIsLeft ? block.LeftCount : block.RightCount;
        int tStart = sourceIsLeft ? block.RightStart : block.LeftStart;
        int tCount = sourceIsLeft ? block.RightCount : block.LeftCount;

        string text = sCount == 0 ? "" : NormalizeNewLines(
            source.GetText(source.LineStart(sStart), source.LineEnd(sStart + sCount - 1) - source.LineStart(sStart)), targetNewLine);

        if (sCount > 0 && tCount > 0)
        {
            int start = target.LineStart(tStart);
            return new TextEdit(start, target.LineEnd(tStart + tCount - 1) - start, text);
        }

        if (sCount > 0)
        {
            // Insert whole lines before target line tStart, or append after the last line.
            if (tStart < target.LineCount)
                return new TextEdit(target.LineStart(tStart), 0, text + targetNewLine);
            return new TextEdit(target.TextLength, 0, targetNewLine + text);
        }

        if (tCount == 0) return new TextEdit(0, 0, "");

        // Remove target lines together with one line break.
        int tEnd = tStart + tCount;
        if (tEnd < target.LineCount)
        {
            int start = target.LineStart(tStart);
            return new TextEdit(start, target.LineStart(tEnd) - start, "");
        }
        if (tStart > 0)
        {
            int start = target.LineEnd(tStart - 1);
            return new TextEdit(start, target.TextLength - start, "");
        }
        return new TextEdit(0, target.TextLength, "");
    }

    public static string NormalizeNewLines(string text, string newLine)
    {
        if (text.IndexOfAny(new[] { '\r', '\n' }) < 0) return text;
        return string.Join(newLine, TextLines.Split(text));
    }
}
