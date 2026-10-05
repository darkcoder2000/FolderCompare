using System.IO.Abstractions.TestingHelpers;
using FolderCompare.Core.Comparison;
using FolderCompare.Core.Models;

namespace FolderCompare.Core.Tests;

internal static class TestFs
{
    public const string L = @"C:\left";
    public const string R = @"C:\right";
    public static readonly DateTime T0 = new(2024, 5, 1, 12, 0, 0, DateTimeKind.Utc);

    public static MockFileSystem Create()
    {
        var fs = new MockFileSystem();
        fs.Directory.CreateDirectory(L);
        fs.Directory.CreateDirectory(R);
        return fs;
    }

    public static void AddFile(this MockFileSystem fs, string path, DateTime lastWriteUtc, string content = "data")
    {
        fs.AddFile(path, new MockFileData(content) { LastWriteTime = lastWriteUtc.ToLocalTime() });
        fs.File.SetLastWriteTimeUtc(path, lastWriteUtc);
    }

    public static DiffResult Compare(this MockFileSystem fs, CompareOptions? options = null) =>
        new DiffEngine(fs).CompareAsync(L, R, options ?? new CompareOptions()).GetAwaiter().GetResult();

    public static DiffNode Find(this DiffResult result, string relativePath) =>
        result.Root.Descendants().Single(n => string.Equals(n.RelativePath, relativePath, StringComparison.OrdinalIgnoreCase));

    public static DiffNode? TryFind(this DiffResult result, string relativePath) =>
        result.Root.Descendants().SingleOrDefault(n => string.Equals(n.RelativePath, relativePath, StringComparison.OrdinalIgnoreCase));
}
