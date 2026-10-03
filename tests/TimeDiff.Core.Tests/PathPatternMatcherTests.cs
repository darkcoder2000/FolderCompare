using TimeDiff.Core.Comparison;

namespace TimeDiff.Core.Tests;

public class PathPatternMatcherTests
{
    [Theory]
    [InlineData("*.tmp", "file.tmp", @"a\file.tmp", true)]
    [InlineData("*.tmp", "file.TMP", @"file.TMP", true)]
    [InlineData("*.tmp", "file.tmpx", @"file.tmpx", false)]
    [InlineData("node_modules", "node_modules", @"src\node_modules", true)]
    [InlineData("node_modules", "node_modules2", @"node_modules2", false)]
    [InlineData("bin;obj", "obj", @"x\obj", true)]
    [InlineData(" bin ; obj ", "bin", @"bin", true)]
    [InlineData("file?.txt", "file1.txt", @"file1.txt", true)]
    [InlineData("file?.txt", "file12.txt", @"file12.txt", false)]
    [InlineData(@"src\gen", "gen", @"src\gen", true)]
    [InlineData(@"src\gen", "gen", @"other\gen", false)]
    [InlineData("src/*/tmp", "tmp", @"src\a\tmp", true)]
    [InlineData(@"**\cache", "cache", @"a\b\cache", true)]
    [InlineData("a+b(1).txt", "a+b(1).txt", "a+b(1).txt", true)]
    [InlineData("", "anything", "anything", false)]
    public void Exclude(string patterns, string name, string rel, bool excluded) =>
        Assert.Equal(excluded, new PathPatternMatcher(null, patterns).IsExcluded(name, rel));

    [Fact]
    public void EmptyInclude_IncludesEverything() =>
        Assert.True(new PathPatternMatcher("", null).IsIncludedFile("x.bin", "x.bin"));

    [Fact]
    public void Include_RestrictsFiles()
    {
        var m = new PathPatternMatcher("*.cs;*.xaml", null);
        Assert.True(m.IsIncludedFile("A.CS", "A.CS"));
        Assert.True(m.IsIncludedFile("Main.xaml", @"v\Main.xaml"));
        Assert.False(m.IsIncludedFile("readme.md", "readme.md"));
    }
}
