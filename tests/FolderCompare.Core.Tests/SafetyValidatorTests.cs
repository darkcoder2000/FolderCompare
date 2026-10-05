using FolderCompare.Core.Operations;
using static FolderCompare.Core.Tests.TestFs;

namespace FolderCompare.Core.Tests;

public class SafetyValidatorTests
{
    [Fact]
    public void ValidRoots() => Assert.Null(SafetyValidator.ValidateRoots(Create(), L, R));

    [Fact]
    public void SameRoot_IsRejected()
    {
        var fs = Create();
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, L, L));
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, L, L.ToUpperInvariant() + "\\"));
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, L, $@"{R}\..\left"));
    }

    [Fact]
    public void NestedRoot_IsRejected()
    {
        var fs = Create();
        fs.Directory.CreateDirectory($@"{L}\sub");
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, L, $@"{L}\sub"));
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, $@"{L}\sub", L));
    }

    [Fact]
    public void SimilarPrefix_IsNotNested()
    {
        var fs = Create();
        fs.Directory.CreateDirectory(@"C:\left2");
        Assert.Null(SafetyValidator.ValidateRoots(fs, L, @"C:\left2"));
    }

    [Fact]
    public void MissingRoot_IsRejected() => Assert.NotNull(SafetyValidator.ValidateRoots(Create(), L, @"C:\nope"));

    [Fact]
    public void EmptyRoot_IsRejected() => Assert.NotNull(SafetyValidator.ValidateRoots(Create(), L, " "));

    [Theory]
    [InlineData(@"C:\left\a.txt", true)]
    [InlineData(@"C:\left\sub\..\a.txt", true)]
    [InlineData(@"C:\left\..\right\a.txt", false)]
    [InlineData(@"C:\left\..\left2\a.txt", false)]
    [InlineData(@"C:\left", false)]
    [InlineData(@"C:\left\", false)]
    [InlineData(@"C:\other\a.txt", false)]
    public void EnsureInsideRoot(string path, bool allowed)
    {
        var fs = Create();
        if (allowed) Assert.NotNull(SafetyValidator.EnsureInsideRoot(fs, L, path));
        else Assert.Throws<UnsafePathException>(() => SafetyValidator.EnsureInsideRoot(fs, L, path));
    }
}
