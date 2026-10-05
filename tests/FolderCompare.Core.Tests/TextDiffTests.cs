using System.IO.Abstractions.TestingHelpers;
using System.Text;
using FolderCompare.Core.TextDiff;

namespace FolderCompare.Core.Tests;

public class LineDifferTests
{
    private static readonly TextDiffOptions Exact = new();

    private static IReadOnlyList<DiffBlock> Diff(string left, string right, TextDiffOptions? o = null) =>
        LineDiffer.Compare(TextLines.Split(left), TextLines.Split(right), o ?? Exact);

    [Fact]
    public void Identical_IsOneEqualBlock()
    {
        var blocks = Diff("a\nb\nc", "a\nb\nc");
        Assert.Equal(new[] { new DiffBlock(0, 3, 0, 3, DiffBlockKind.Equal) }, blocks);
    }

    [Fact]
    public void InsertedLine_IsRightOnly()
    {
        var blocks = Diff("a\nc", "a\nb\nc");
        Assert.Contains(new DiffBlock(1, 0, 1, 1, DiffBlockKind.RightOnly), blocks);
        Assert.Single(blocks, b => b.IsDifference);
    }

    [Fact]
    public void DeletedLine_IsLeftOnly()
    {
        var blocks = Diff("a\nb\nc", "a\nc");
        Assert.Contains(new DiffBlock(1, 1, 1, 0, DiffBlockKind.LeftOnly), blocks);
    }

    [Fact]
    public void ChangedLines_AreChanged()
    {
        var blocks = Diff("a\nb\nc\nd", "a\nX\nY\nd");
        Assert.Contains(new DiffBlock(1, 2, 1, 2, DiffBlockKind.Changed), blocks);
    }

    [Fact]
    public void EmptyVersusText()
    {
        var blocks = Diff("", "x\ny");
        Assert.Equal(new[] { new DiffBlock(0, 1, 0, 2, DiffBlockKind.Changed) }, blocks);
    }

    [Fact]
    public void MissingTrailingNewline_IsADifference()
    {
        var blocks = Diff("a\nb\n", "a\nb");
        Assert.Contains(blocks, b => b.IsDifference);
    }

    [Fact]
    public void WhitespaceOnlyChange_IsUnimportantWhenIgnored()
    {
        Assert.Contains(Diff("a\n  b  c\nd", "a\nb c\nd"), b => b.Kind == DiffBlockKind.Changed);
        var blocks = Diff("a\n  b  c\nd", "a\nb c\nd", new TextDiffOptions { IgnoreWhitespace = true });
        Assert.Contains(new DiffBlock(1, 1, 1, 1, DiffBlockKind.Unimportant), blocks);
        Assert.DoesNotContain(blocks, b => b.IsDifference);
    }

    [Fact]
    public void CaseOnlyChange_IsUnimportantWhenIgnored()
    {
        var blocks = Diff("Hello", "hello", new TextDiffOptions { IgnoreCase = true });
        Assert.Equal(new[] { new DiffBlock(0, 1, 0, 1, DiffBlockKind.Unimportant) }, blocks);
    }

    [Fact]
    public void BlocksCoverBothSidesContiguously()
    {
        var blocks = Diff("1\n2\n3\n4\n5\n6", "0\n2\n3\nx\n5\n7\n8");
        int l = 0, r = 0;
        foreach (var b in blocks)
        {
            Assert.Equal(l, b.LeftStart);
            Assert.Equal(r, b.RightStart);
            l += b.LeftCount;
            r += b.RightCount;
        }
        Assert.Equal(6, l);
        Assert.Equal(7, r);
    }

    [Fact]
    public void RandomEdits_CopyingEveryBlockToTheLeftReproducesTheRight()
    {
        var rnd = new Random(1234);
        for (int round = 0; round < 200; round++)
        {
            var left = Enumerable.Range(0, rnd.Next(0, 30)).Select(_ => ((char)('a' + rnd.Next(6))).ToString()).ToList();
            var right = left.ToList();
            for (int e = rnd.Next(0, 8); e > 0; e--)
            {
                int op = rnd.Next(3);
                if (op == 0 || right.Count == 0) right.Insert(rnd.Next(right.Count + 1), ((char)('a' + rnd.Next(8))).ToString());
                else if (op == 1) right.RemoveAt(rnd.Next(right.Count));
                else right[rnd.Next(right.Count)] = ((char)('a' + rnd.Next(8))).ToString();
            }

            var leftText = string.Join("\n", left);
            var rightText = string.Join("\n", right);
            var blocks = LineDiffer.Compare(TextLines.Split(leftText), TextLines.Split(rightText), Exact);

            // Apply from the last block to the first so earlier line numbers stay valid.
            var text = leftText;
            var rightSource = new StringLineSource(rightText);
            foreach (var b in blocks.Where(b => b.IsDifference).Reverse())
            {
                var edit = BlockCopy.Compute(rightSource, new StringLineSource(text), b, sourceIsLeft: false, "\n");
                text = text.Remove(edit.Offset, edit.Length).Insert(edit.Offset, edit.Text);
            }
            Assert.Equal(rightText, text);
        }
    }

    [Fact]
    public void LargeDissimilarFiles_FinishQuickly()
    {
        var left = Enumerable.Range(0, 20000).Select(n => "L" + n).ToList();
        var right = Enumerable.Range(0, 20000).Select(n => n % 3 == 0 ? "L" + n : "R" + n).ToList();
        var sw = System.Diagnostics.Stopwatch.StartNew();
        var blocks = LineDiffer.Compare(left, right, Exact);
        Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), sw.Elapsed.ToString());
        Assert.Equal(20000, blocks.Sum(b => b.LeftCount));
    }
}

public class InlineDifferTests
{
    [Fact]
    public void ChangedWord_IsHighlightedOnBothSides()
    {
        var (l, r) = InlineDiffer.Compare("int count = 5;", "int total = 5;");
        Assert.Equal(new[] { new TextRange(4, 5) }, l);
        Assert.Equal(new[] { new TextRange(4, 5) }, r);
    }

    [Fact]
    public void InsertedWord_IsOnlyOnOneSide()
    {
        var (l, r) = InlineDiffer.Compare("a c", "a b c");
        Assert.Empty(l);
        Assert.Equal("b ", "a b c".Substring(r[0].Start, r[0].Length));
    }

    [Fact]
    public void WhitespaceRanges_AreDroppedWhenIgnored()
    {
        var (l, r) = InlineDiffer.Compare("a  b", "a b", new TextDiffOptions { IgnoreWhitespace = true });
        Assert.Empty(l);
        Assert.Empty(r);
    }

    [Fact]
    public void Tokenize_SplitsWordsSpacesAndPunctuation()
    {
        var tokens = InlineDiffer.Tokenize("ab  c();");
        Assert.Equal(new[] { "ab", "  ", "c", "(", ")", ";" }, tokens.Select(t => "ab  c();".Substring(t.Start, t.Length)));
    }
}

public class TextFileCodecTests
{
    private static LoadedText RoundTrip(byte[] bytes, out byte[] saved)
    {
        var fs = new MockFileSystem();
        fs.AddFile(@"C:\t.txt", new MockFileData(bytes));
        var loaded = TextFileCodec.Load(fs, @"C:\t.txt");
        TextFileCodec.Save(fs, @"C:\t2.txt", loaded.Text, loaded);
        saved = fs.File.ReadAllBytes(@"C:\t2.txt");
        return loaded;
    }

    [Fact]
    public void Utf8WithoutBom()
    {
        var bytes = Encoding.UTF8.GetBytes("Grüße\r\nWelt");
        var t = RoundTrip(bytes, out var saved);
        Assert.Equal("Grüße\r\nWelt", t.Text);
        Assert.Equal("UTF-8", t.EncodingName);
        Assert.Equal("\r\n", t.NewLine);
        Assert.Equal(bytes, saved);
    }

    [Fact]
    public void Utf8WithBom_KeepsBom()
    {
        var bytes = new byte[] { 0xEF, 0xBB, 0xBF }.Concat(Encoding.UTF8.GetBytes("x\ny")).ToArray();
        var t = RoundTrip(bytes, out var saved);
        Assert.Equal("x\ny", t.Text);
        Assert.Equal("UTF-8 BOM", t.EncodingName);
        Assert.Equal("\n", t.NewLine);
        Assert.Equal(bytes, saved);
    }

    [Fact]
    public void Utf16LittleEndian()
    {
        var bytes = new byte[] { 0xFF, 0xFE }.Concat(Encoding.Unicode.GetBytes("hällo")).ToArray();
        var t = RoundTrip(bytes, out var saved);
        Assert.Equal("hällo", t.Text);
        Assert.False(t.IsBinary);
        Assert.Equal(bytes, saved);
    }

    [Fact]
    public void InvalidUtf8_FallsBackToAnsi()
    {
        var bytes = new byte[] { (byte)'G', 0xFC, (byte)'t', 0x80 }; // "Güt€" in Windows-1252
        var t = RoundTrip(bytes, out var saved);
        Assert.Equal("Güt€", t.Text);
        Assert.Equal("ANSI", t.EncodingName);
        Assert.Equal(bytes, saved);
    }

    [Fact]
    public void NulByte_IsBinary()
    {
        var t = TextFileCodec.Decode(new byte[] { 1, 2, 0, 4 });
        Assert.True(t.IsBinary);
    }

    [Theory]
    [InlineData("a\nb\nc\r\n", "\n")]
    [InlineData("a\r\nb\r\n", "\r\n")]
    [InlineData("a\rb\r", "\r")]
    [InlineData("single line", "\r\n")]
    public void DetectsDominantNewLine(string text, string expected) =>
        Assert.Equal(expected, TextFileCodec.DetectNewLine(text));
}

public class BlockCopyTests
{
    private static string Apply(string source, string target, bool sourceIsLeft, string nl = "\n")
    {
        var left = sourceIsLeft ? source : target;
        var right = sourceIsLeft ? target : source;
        var blocks = LineDiffer.Compare(TextLines.Split(left), TextLines.Split(right), new TextDiffOptions());
        var block = Assert.Single(blocks, b => b.IsDifference);
        var edit = BlockCopy.Compute(new StringLineSource(source), new StringLineSource(target), block, sourceIsLeft, nl);
        return target.Remove(edit.Offset, edit.Length).Insert(edit.Offset, edit.Text);
    }

    [Theory]
    [InlineData("x\na\nb", "a\nb")]          // insert at start
    [InlineData("a\nx\nb", "a\nb")]          // insert in the middle
    [InlineData("a\nb\nx", "a\nb")]          // append after a last line without newline
    [InlineData("a\nb\nx\n", "a\nb\n")]      // insert before the final empty line
    [InlineData("a\nb", "a\nx\nb")]          // delete in the middle
    [InlineData("a\nb", "a\nb\nx")]          // delete at the end
    [InlineData("b", "x\nb")]                // delete at the start
    [InlineData("a\nX\nY\nb", "a\nq\nb")]    // replace
    [InlineData("", "x\ny")]                 // copy an empty file over
    [InlineData("x\ny", "")]                 // copy into an empty file
    public void CopyMakesTargetEqualToSource(string source, string target)
    {
        Assert.Equal(source, Apply(source, target, sourceIsLeft: true));
        Assert.Equal(source, Apply(source, target, sourceIsLeft: false));
    }

    [Fact]
    public void CopiedLineBreaks_AreConvertedToTargetNewLine()
    {
        var result = Apply("a\nX\nY\nb", "a\r\nq\r\nb", sourceIsLeft: true, "\r\n");
        Assert.Equal("a\r\nX\r\nY\r\nb", result);
    }
}

public class TextComparisonTests
{
    [Fact]
    public void Fillers_MakeBothSidesTheSameHeight()
    {
        var c = TextComparison.Compute("a\nb\nc", "a\nX\nY\nZ\nc\nd", new TextDiffOptions());
        int Rows(TextComparison.Side s) => s.LineCount + s.FillerAbove + s.FillerBelow.Values.Sum();
        Assert.Equal(c.TotalRows, Rows(c.Left));
        Assert.Equal(c.TotalRows, Rows(c.Right));
        Assert.Equal(2, c.Left.FillerBelow[1]);   // "b" vs "X Y Z": two filler rows below b
        Assert.Equal(1, c.Left.FillerBelow[2]);   // "d" only on the right, after "c"
    }

    [Fact]
    public void InsertAtTop_UsesFillerAbove()
    {
        var c = TextComparison.Compute("b", "a\nb", new TextDiffOptions());
        Assert.Equal(1, c.Left.FillerAbove);
        Assert.Empty(c.Left.FillerBelow);
    }

    [Fact]
    public void PartnerLine_FollowsAlignment()
    {
        var c = TextComparison.Compute("a\nb\nc", "a\nc", new TextDiffOptions());
        Assert.Equal(0, c.PartnerLine(true, 0));
        Assert.Null(c.PartnerLine(true, 1));
        Assert.Equal(1, c.PartnerLine(true, 2));
        Assert.Equal(2, c.PartnerLine(false, 1));
    }

    [Fact]
    public void ChangedLines_GetInlineRanges()
    {
        var c = TextComparison.Compute("x = 1", "x = 2", new TextDiffOptions());
        Assert.Equal(new[] { new TextRange(4, 1) }, c.Left.Inline[0]);
        Assert.Equal(DiffBlockKind.Changed, c.Right.Kinds[0]);
        Assert.Equal(1, c.DifferenceCount);
    }
}
