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
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, L, P($@"{R}\..\left")));
    }

    [Fact]
    public void NestedRoot_IsRejected()
    {
        var fs = Create();
        fs.Directory.CreateDirectory(P($@"{L}\sub"));
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, L, P($@"{L}\sub")));
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, P($@"{L}\sub"), L));
    }

    [Fact]
    public void SimilarPrefix_IsNotNested()
    {
        var fs = Create();
        fs.Directory.CreateDirectory(P(@"C:\left2"));
        Assert.Null(SafetyValidator.ValidateRoots(fs, L, P(@"C:\left2")));
    }

    [Fact]
    public void MissingRoot_IsRejected() => Assert.NotNull(SafetyValidator.ValidateRoots(Create(), L, P(@"C:\nope")));

    [Fact]
    public void EmptyRoot_IsRejected() => Assert.NotNull(SafetyValidator.ValidateRoots(Create(), L, " "));

    [Fact]
    public void EnsureInsideRoot_DoesNotIgnoreCase()
    {
        // A differently cased root may be another folder on a case-sensitive file system, so it is never accepted.
        var fs = Create();
        Assert.Throws<UnsafePathException>(() => SafetyValidator.EnsureInsideRoot(fs, L, P(@"C:\LEFT\a.txt")));
    }

    [Fact]
    public void ValidateRoots_IgnoresCaseForNesting()
    {
        var fs = Create();
        fs.Directory.CreateDirectory(P(@"C:\left\Sub"));
        if (!OperatingSystem.IsLinux()) Assert.NotNull(SafetyValidator.ValidateRoots(fs, P(@"C:\LEFT\Sub"), L));
        Assert.NotNull(SafetyValidator.ValidateRoots(fs, P(@"C:\left\Sub"), L));
    }

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
        path = P(path);
        if (allowed) Assert.NotNull(SafetyValidator.EnsureInsideRoot(fs, L, path));
        else Assert.Throws<UnsafePathException>(() => SafetyValidator.EnsureInsideRoot(fs, L, path));
    }
}
