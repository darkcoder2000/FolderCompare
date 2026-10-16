using System.IO.Abstractions.TestingHelpers;
using FolderCompare.Core.Comparison;
using FolderCompare.Core.Models;
using static FolderCompare.Core.Tests.TestFs;

namespace FolderCompare.Core.Tests;

public class DiffEngineTests
{
    [Fact]
    public void ClassifiesAllFileStatuses()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\same.txt"), T0);
        fs.AddFile(P($@"{R}\same.txt"), T0.AddSeconds(1));
        fs.AddFile(P($@"{L}\newleft.txt"), T0.AddMinutes(5));
        fs.AddFile(P($@"{R}\newleft.txt"), T0);
        fs.AddFile(P($@"{L}\newright.txt"), T0);
        fs.AddFile(P($@"{R}\newright.txt"), T0.AddMinutes(5));
        fs.AddFile(P($@"{L}\onlyleft.txt"), T0);
        fs.AddFile(P($@"{R}\onlyright.txt"), T0);

        var r = fs.Compare();

        Assert.Equal(DiffStatus.Identical, r.Find("same.txt").Status);
        Assert.Equal(DiffStatus.NewerLeft, r.Find("newleft.txt").Status);
        Assert.Equal(DiffStatus.NewerRight, r.Find("newright.txt").Status);
        Assert.Equal(DiffStatus.OnlyLeft, r.Find("onlyleft.txt").Status);
        Assert.Equal(DiffStatus.OnlyRight, r.Find("onlyright.txt").Status);
        Assert.Null(r.Find("onlyleft.txt").Right);
        Assert.Null(r.Find("onlyright.txt").Left);
    }

    [Fact]
    public void SameTimeDifferentSize_IsDetectedWithoutContentCompare()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\d\a.txt"), T0, "short");
        fs.AddFile(P($@"{R}\d\a.txt"), T0, "a much longer content");
        var r = fs.Compare();
        Assert.Equal(DiffStatus.SameTimeDifferentContent, r.Find(P(@"d\a.txt")).Status);
        Assert.Equal(DiffStatus.Differs, r.Find("d").Status);
        Assert.Equal(DiffStatus.Differs, r.Root.Status);
    }

    [Fact]
    public void DifferentTimeDifferentSize_KeepsDirection()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\a.txt"), T0.AddMinutes(5), "short");
        fs.AddFile(P($@"{R}\a.txt"), T0, "a much longer content");
        Assert.Equal(DiffStatus.NewerLeft, fs.Compare().Find("a.txt").Status);
    }

    [Fact]
    public void ContentCompare_SameTimeSameSizeDifferentContent()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\d\a.txt"), T0, "abc");
        fs.AddFile(P($@"{R}\d\a.txt"), T0, "xyz");
        fs.AddFile(P($@"{L}\d\b.txt"), T0, "same");
        fs.AddFile(P($@"{R}\d\b.txt"), T0, "same");

        Assert.Equal(DiffStatus.Identical, fs.Compare().Find(P(@"d\a.txt")).Status);

        var r = fs.Compare(new CompareOptions { ContentCompare = ContentCompareMode.WhenTimestampsEqual });
        Assert.Equal(DiffStatus.SameTimeDifferentContent, r.Find(P(@"d\a.txt")).Status);
        Assert.Equal(DiffStatus.Identical, r.Find(P(@"d\b.txt")).Status);
        Assert.Equal(DiffStatus.Differs, r.Find("d").Status);
        Assert.Equal(DiffStatus.Differs, r.Root.Status);
    }

    [Fact]
    public void ContentCompare_TimeOnlyDifferences_RollUpAsIdentical()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\d\a.txt"), T0.AddDays(1), "abc");
        fs.AddFile(P($@"{R}\d\a.txt"), T0, "abc");
        fs.AddFile(P($@"{L}\d\b.txt"), T0, "same");
        fs.AddFile(P($@"{R}\d\b.txt"), T0, "same");

        var r = fs.Compare(new CompareOptions { ContentCompare = ContentCompareMode.WhenSizesEqual });

        Assert.Equal(DiffStatus.SameContentDifferentTime, r.Find(P(@"d\a.txt")).Status);
        Assert.Equal(DiffStatus.Identical, r.Find("d").Status);
        Assert.Equal(DiffStatus.Identical, r.Root.Status);
    }

    [Fact]
    public void ContentCompare_LockedFile_IsErrorWithMessage()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\a.txt"), T0, "abc");
        fs.AddFile(P($@"{R}\a.txt"), T0, "abc");
        fs.GetFile(P($@"{R}\a.txt")).AllowedFileShare = FileShare.None;

        var node = fs.Compare(new CompareOptions { ContentCompare = ContentCompareMode.WhenTimestampsEqual }).Find("a.txt");

        Assert.Equal(DiffStatus.Error, node.Status);
        Assert.StartsWith("Content compare failed:", node.ErrorMessage);
    }

    [Fact]
    public async Task ContentCompare_ReportsBytes()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\a.txt"), T0, "abcd");
        fs.AddFile(P($@"{R}\a.txt"), T0, "abcd");
        fs.AddFile(P($@"{L}\b.txt"), T0, "123456");
        fs.AddFile(P($@"{R}\b.txt"), T0, "654321");
        var reports = new List<ScanProgress>();
        var options = new CompareOptions { ContentCompare = ContentCompareMode.WhenTimestampsEqual };

        await new DiffEngine(fs).CompareAsync(L, R, options, new SyncProgress<ScanProgress>(reports.Add));

        Assert.Contains(reports, p => p.BytesTotal == 10);
        Assert.Equal(4, reports.Last().ItemsScanned);
    }

    [Fact]
    public void Refresh_AppliesContentCompare()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\d\a.txt"), T0, "abc");
        fs.AddFile(P($@"{R}\d\a.txt"), T0, "abc");
        var r = fs.Compare(new CompareOptions { ContentCompare = ContentCompareMode.WhenTimestampsEqual });
        Assert.Equal(DiffStatus.Identical, r.Root.Status);

        fs.File.WriteAllText(P($@"{R}\d\a.txt"), "xyz");
        fs.File.SetLastWriteTimeUtc(P($@"{R}\d\a.txt"), T0);
        var node = r.Find(P(@"d\a.txt"));
        DiffEngine.ApplyRefresh(new DiffEngine(fs).PrepareRefresh(r, node.Parent!, node.Name));

        Assert.Equal(DiffStatus.SameTimeDifferentContent, r.Find(P(@"d\a.txt")).Status);
        Assert.Equal(DiffStatus.Differs, r.Find("d").Status);
        Assert.Equal(DiffStatus.Differs, r.Root.Status);
    }

    [Theory]
    [InlineData(2000, DiffStatus.Identical)]
    [InlineData(1999, DiffStatus.Identical)]
    [InlineData(2001, DiffStatus.NewerLeft)]
    public void ToleranceEdgeCases(int ms, DiffStatus expected)
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\a.txt"), T0.AddMilliseconds(ms));
        fs.AddFile(P($@"{R}\a.txt"), T0);
        Assert.Equal(expected, fs.Compare().Find("a.txt").Status);
    }

    [Fact]
    public void DstOption()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\a.txt"), T0.AddHours(1));
        fs.AddFile(P($@"{R}\a.txt"), T0);
        Assert.Equal(DiffStatus.NewerLeft, fs.Compare().Find("a.txt").Status);
        Assert.Equal(DiffStatus.Identical, fs.Compare(new CompareOptions { IgnoreDstOffset = true }).Find("a.txt").Status);
    }

    [Fact]
    public void PathMatchingIsCaseInsensitive()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\Docs\Readme.TXT"), T0);
        fs.AddFile(P($@"{R}\DOCS\readme.txt"), T0);
        var r = fs.Compare(new CompareOptions { PathComparison = PathComparison.IgnoreCase });
        Assert.Single(r.Root.Children);
        var file = Assert.Single(r.Root.Children[0].Children);
        Assert.Equal(DiffStatus.Identical, file.Status);
        Assert.NotNull(file.Left);
        Assert.NotNull(file.Right);
    }

    [Fact]
    public void PathMatching_CaseSensitive_KeepsNamesApart()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\Docs\Readme.TXT"), T0);
        fs.AddFile(P($@"{R}\DOCS\readme.txt"), T0);
        var r = fs.Compare(new CompareOptions { PathComparison = PathComparison.CaseSensitive });
        Assert.Equal(2, r.Root.Children.Count);
        Assert.Equal(DiffStatus.OnlyLeft, r.Root.Children.Single(c => c.Name == "Docs").Status);
        Assert.Equal(DiffStatus.OnlyRight, r.Root.Children.Single(c => c.Name == "DOCS").Status);
    }

    [Fact]
    public void PathComparison_DefaultFollowsTheFileSystem()
    {
        var fs = Create();
        var r = fs.Compare();
        // The mock file system ignores case only on Windows; on Linux and macOS it is case-sensitive. On macOS the OS
        // default (ignore case) is therefore overridden by the probe of the roots.
        Assert.Equal(OperatingSystem.IsWindows(), r.Options.Paths.IsIgnoreCase);
        Assert.Equal(OperatingSystem.IsWindows(), PathComparison.ForRoots(fs, L, R).IsIgnoreCase);
        Assert.Equal(!OperatingSystem.IsLinux(), PathComparison.ForCurrentOS().IsIgnoreCase);
    }

    [Fact]
    public void CaseOnlyNamesInOneFolder_AreTwoRowsOnCaseSensitiveFileSystems()
    {
        if (OperatingSystem.IsWindows()) return; // needs a case-sensitive (mock) file system: Linux and macOS
        var fs = Create();
        fs.AddFile(P($@"{L}\a.txt"), T0, "lower");
        fs.AddFile(P($@"{L}\A.txt"), T0, "upper");
        fs.AddFile(P($@"{R}\a.txt"), T0, "lower");
        var r = fs.Compare();
        Assert.Equal(2, r.Root.Children.Count);
        Assert.Equal(DiffStatus.Identical, r.Root.Children.Single(c => c.Name == "a.txt").Status);
        Assert.Equal(DiffStatus.OnlyLeft, r.Root.Children.Single(c => c.Name == "A.txt").Status);
    }

    [Fact]
    public void TypeConflict_FileVsFolder()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\thing"), T0);
        fs.Directory.CreateDirectory(P($@"{R}\thing"));
        fs.AddFile(P($@"{R}\thing\inner.txt"), T0);
        var node = fs.Compare().Find("thing");
        Assert.Equal(DiffStatus.TypeConflict, node.Status);
        Assert.NotNull(node.ErrorMessage);
        Assert.Equal(DiffStatus.Differs, fs.Compare().Root.Status);
    }

    [Fact]
    public void FolderRollUp()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\same\a.txt"), T0);
        fs.AddFile(P($@"{R}\same\a.txt"), T0);
        fs.AddFile(P($@"{L}\same\deep\b.txt"), T0);
        fs.AddFile(P($@"{R}\same\deep\b.txt"), T0);
        fs.AddFile(P($@"{L}\diff\a.txt"), T0);
        fs.AddFile(P($@"{R}\diff\a.txt"), T0);
        fs.AddFile(P($@"{L}\diff\deep\b.txt"), T0.AddHours(3));
        fs.AddFile(P($@"{R}\diff\deep\b.txt"), T0);
        fs.AddFile(P($@"{L}\onlyl\x.txt"), T0);
        fs.Directory.CreateDirectory(P($@"{L}\empty"));
        fs.Directory.CreateDirectory(P($@"{R}\empty"));

        var r = fs.Compare();
        Assert.Equal(DiffStatus.Identical, r.Find("same").Status);
        Assert.Equal(DiffStatus.Identical, r.Find(P(@"same\deep")).Status);
        Assert.Equal(DiffStatus.Differs, r.Find("diff").Status);
        Assert.Equal(DiffStatus.Differs, r.Find(P(@"diff\deep")).Status);
        Assert.Equal(DiffStatus.OnlyLeft, r.Find("onlyl").Status);
        Assert.Equal(DiffStatus.OnlyLeft, r.Find(P(@"onlyl\x.txt")).Status);
        Assert.Equal(DiffStatus.Identical, r.Find("empty").Status);
        Assert.Equal(DiffStatus.Differs, r.Root.Status);
    }

    [Fact]
    public void FolderSizesAreSummed()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\d\a.txt"), T0, "12345");
        fs.AddFile(P($@"{L}\d\e\b.txt"), T0, "123");
        var d = fs.Compare().Find("d");
        Assert.Equal(8, d.LeftTotalSize);
        Assert.Equal(0, d.RightTotalSize);
    }

    [Fact]
    public void ExcludePatterns_SkipFilesAndFolders()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\keep.txt"), T0);
        fs.AddFile(P($@"{L}\junk.tmp"), T0);
        fs.AddFile(P($@"{L}\node_modules\pkg\index.js"), T0);
        fs.AddFile(P($@"{R}\obj\x.dll"), T0);
        var r = fs.Compare(new CompareOptions { ExcludePatterns = "*.tmp;node_modules;obj" });
        Assert.NotNull(r.TryFind("keep.txt"));
        Assert.Null(r.TryFind("junk.tmp"));
        Assert.Null(r.TryFind("node_modules"));
        Assert.Null(r.TryFind("obj"));
    }

    [Fact]
    public void IncludePatterns_RestrictFilesButKeepFolders()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\src\a.cs"), T0);
        fs.AddFile(P($@"{L}\src\b.txt"), T0);
        var r = fs.Compare(new CompareOptions { IncludePatterns = "*.cs" });
        Assert.NotNull(r.TryFind(P(@"src\a.cs")));
        Assert.Null(r.TryFind(P(@"src\b.txt")));
    }

    [Fact]
    public void IgnoreHiddenAndSystem()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\visible.txt"), T0);
        fs.AddFile(P($@"{L}\hidden.txt"), T0);
        fs.File.SetAttributes(P($@"{L}\hidden.txt"), FileAttributes.Hidden);
        Assert.NotNull(fs.Compare().TryFind("hidden.txt"));
        var r = fs.Compare(new CompareOptions { IgnoreHiddenAndSystem = true });
        Assert.Null(r.TryFind("hidden.txt"));
        Assert.NotNull(r.TryFind("visible.txt"));
    }

    [Fact]
    public void NonRecursive_OnlyTopLevel()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\top.txt"), T0);
        fs.AddFile(P($@"{L}\sub\deep.txt"), T0);
        var r = fs.Compare(new CompareOptions { Recursive = false });
        Assert.NotNull(r.TryFind("top.txt"));
        Assert.Empty(r.Find("sub").Children);
    }

    [Fact]
    public void SymlinkLoop_IsDetectedWhenFollowing()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\a\file.txt"), T0);
        fs.Directory.CreateSymbolicLink(P($@"{L}\a\loop"), P($@"{L}\a"));

        var r = fs.Compare(new CompareOptions { SymlinkMode = SymlinkMode.Follow });
        var loop = r.Find(P(@"a\loop"));
        Assert.Equal(DiffStatus.Error, loop.Status);
        Assert.Contains("loop", loop.ErrorMessage, StringComparison.OrdinalIgnoreCase);
        Assert.Empty(loop.Children);
    }

    [Fact]
    public void Symlinks_SkippedByDefault()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\target\file.txt"), T0);
        fs.Directory.CreateSymbolicLink(P($@"{L}\link"), P($@"{L}\target"));
        var r = fs.Compare();
        Assert.Null(r.TryFind("link"));
        Assert.NotNull(r.TryFind(P(@"target\file.txt")));
    }

    [Fact]
    public void Scan_DoesNotModifyFileSystem()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\a\b.txt"), T0);
        fs.AddFile(P($@"{R}\c.txt"), T0);
        var before = fs.AllPaths.OrderBy(p => p).ToList();
        fs.Compare();
        Assert.Equal(before, fs.AllPaths.OrderBy(p => p).ToList());
    }

    [Fact]
    public async Task Scan_CanBeCancelled()
    {
        var fs = Create();
        for (int i = 0; i < 50; i++) fs.AddFile(P($@"{L}\d{i}\f.txt"), T0);
        using var cts = new CancellationTokenSource();
        cts.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            new DiffEngine(fs).CompareAsync(L, R, new CompareOptions(), null, cts.Token));
    }

    [Fact]
    public async Task Scan_ReportsProgress()
    {
        var fs = Create();
        for (int i = 0; i < 10; i++) fs.AddFile(P($@"{L}\f{i}.txt"), T0);
        var reports = new List<ScanProgress>();
        var progress = new SyncProgress<ScanProgress>(reports.Add);
        await new DiffEngine(fs).CompareAsync(L, R, new CompareOptions(), progress);
        Assert.Equal(10, reports.Last().ItemsScanned);
    }

    [Fact]
    public async Task RejectsSameOrNestedRoots()
    {
        var fs = Create();
        fs.Directory.CreateDirectory(P($@"{L}\inner"));
        var engine = new DiffEngine(fs);
        await Assert.ThrowsAsync<InvalidOperationException>(() => engine.CompareAsync(L, L + "\\", new CompareOptions()));
        await Assert.ThrowsAsync<InvalidOperationException>(() => engine.CompareAsync(L, P($@"{L}\inner"), new CompareOptions()));
    }

    [Fact]
    public void Refresh_UpdatesNodeAndAncestors()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\d\a.txt"), T0.AddHours(1));
        fs.AddFile(P($@"{R}\d\a.txt"), T0);
        var r = fs.Compare();
        var node = r.Find(P(@"d\a.txt"));
        Assert.Equal(DiffStatus.Differs, r.Root.Status);

        fs.File.SetLastWriteTimeUtc(P($@"{R}\d\a.txt"), T0.AddHours(1));
        var engine = new DiffEngine(fs);
        DiffEngine.ApplyRefresh(engine.PrepareRefresh(r, node.Parent!, node.Name));

        Assert.Equal(DiffStatus.Identical, r.Find(P(@"d\a.txt")).Status);
        Assert.Equal(DiffStatus.Identical, r.Find("d").Status);
        Assert.Equal(DiffStatus.Identical, r.Root.Status);
    }

    [Fact]
    public void Refresh_RemovesDeletedNode()
    {
        var fs = Create();
        fs.AddFile(P($@"{L}\a.txt"), T0);
        var r = fs.Compare();
        fs.File.Delete(P($@"{L}\a.txt"));
        DiffEngine.ApplyRefresh(new DiffEngine(fs).PrepareRefresh(r, r.Root, "a.txt"));
        Assert.Empty(r.Root.Children);
        Assert.Equal(DiffStatus.Identical, r.Root.Status);
    }

    private sealed class SyncProgress<T>(Action<T> handler) : IProgress<T>
    {
        private readonly object _lock = new();
        public void Report(T value)
        {
            lock (_lock) handler(value);
        }
    }
}
