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
        fs.AddFile($@"{L}\same.txt", T0);
        fs.AddFile($@"{R}\same.txt", T0.AddSeconds(1));
        fs.AddFile($@"{L}\newleft.txt", T0.AddMinutes(5));
        fs.AddFile($@"{R}\newleft.txt", T0);
        fs.AddFile($@"{L}\newright.txt", T0);
        fs.AddFile($@"{R}\newright.txt", T0.AddMinutes(5));
        fs.AddFile($@"{L}\onlyleft.txt", T0);
        fs.AddFile($@"{R}\onlyright.txt", T0);

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
    public void SizeIsNotUsedForComparison()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0, "short");
        fs.AddFile($@"{R}\a.txt", T0, "a much longer content");
        var node = fs.Compare().Find("a.txt");
        Assert.Equal(DiffStatus.Identical, node.Status);
        Assert.NotEqual(node.Left!.Size, node.Right!.Size);
    }

    [Theory]
    [InlineData(2000, DiffStatus.Identical)]
    [InlineData(1999, DiffStatus.Identical)]
    [InlineData(2001, DiffStatus.NewerLeft)]
    public void ToleranceEdgeCases(int ms, DiffStatus expected)
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0.AddMilliseconds(ms));
        fs.AddFile($@"{R}\a.txt", T0);
        Assert.Equal(expected, fs.Compare().Find("a.txt").Status);
    }

    [Fact]
    public void DstOption()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0.AddHours(1));
        fs.AddFile($@"{R}\a.txt", T0);
        Assert.Equal(DiffStatus.NewerLeft, fs.Compare().Find("a.txt").Status);
        Assert.Equal(DiffStatus.Identical, fs.Compare(new CompareOptions { IgnoreDstOffset = true }).Find("a.txt").Status);
    }

    [Fact]
    public void PathMatchingIsCaseInsensitive()
    {
        var fs = Create();
        fs.AddFile($@"{L}\Docs\Readme.TXT", T0);
        fs.AddFile($@"{R}\DOCS\readme.txt", T0);
        var r = fs.Compare();
        Assert.Single(r.Root.Children);
        var file = Assert.Single(r.Root.Children[0].Children);
        Assert.Equal(DiffStatus.Identical, file.Status);
        Assert.NotNull(file.Left);
        Assert.NotNull(file.Right);
    }

    [Fact]
    public void TypeConflict_FileVsFolder()
    {
        var fs = Create();
        fs.AddFile($@"{L}\thing", T0);
        fs.Directory.CreateDirectory($@"{R}\thing");
        fs.AddFile($@"{R}\thing\inner.txt", T0);
        var node = fs.Compare().Find("thing");
        Assert.Equal(DiffStatus.TypeConflict, node.Status);
        Assert.NotNull(node.ErrorMessage);
        Assert.Equal(DiffStatus.Differs, fs.Compare().Root.Status);
    }

    [Fact]
    public void FolderRollUp()
    {
        var fs = Create();
        fs.AddFile($@"{L}\same\a.txt", T0);
        fs.AddFile($@"{R}\same\a.txt", T0);
        fs.AddFile($@"{L}\same\deep\b.txt", T0);
        fs.AddFile($@"{R}\same\deep\b.txt", T0);
        fs.AddFile($@"{L}\diff\a.txt", T0);
        fs.AddFile($@"{R}\diff\a.txt", T0);
        fs.AddFile($@"{L}\diff\deep\b.txt", T0.AddHours(3));
        fs.AddFile($@"{R}\diff\deep\b.txt", T0);
        fs.AddFile($@"{L}\onlyl\x.txt", T0);
        fs.Directory.CreateDirectory($@"{L}\empty");
        fs.Directory.CreateDirectory($@"{R}\empty");

        var r = fs.Compare();
        Assert.Equal(DiffStatus.Identical, r.Find("same").Status);
        Assert.Equal(DiffStatus.Identical, r.Find(@"same\deep").Status);
        Assert.Equal(DiffStatus.Differs, r.Find("diff").Status);
        Assert.Equal(DiffStatus.Differs, r.Find(@"diff\deep").Status);
        Assert.Equal(DiffStatus.OnlyLeft, r.Find("onlyl").Status);
        Assert.Equal(DiffStatus.OnlyLeft, r.Find(@"onlyl\x.txt").Status);
        Assert.Equal(DiffStatus.Identical, r.Find("empty").Status);
        Assert.Equal(DiffStatus.Differs, r.Root.Status);
    }

    [Fact]
    public void FolderSizesAreSummed()
    {
        var fs = Create();
        fs.AddFile($@"{L}\d\a.txt", T0, "12345");
        fs.AddFile($@"{L}\d\e\b.txt", T0, "123");
        var d = fs.Compare().Find("d");
        Assert.Equal(8, d.LeftTotalSize);
        Assert.Equal(0, d.RightTotalSize);
    }

    [Fact]
    public void ExcludePatterns_SkipFilesAndFolders()
    {
        var fs = Create();
        fs.AddFile($@"{L}\keep.txt", T0);
        fs.AddFile($@"{L}\junk.tmp", T0);
        fs.AddFile($@"{L}\node_modules\pkg\index.js", T0);
        fs.AddFile($@"{R}\obj\x.dll", T0);
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
        fs.AddFile($@"{L}\src\a.cs", T0);
        fs.AddFile($@"{L}\src\b.txt", T0);
        var r = fs.Compare(new CompareOptions { IncludePatterns = "*.cs" });
        Assert.NotNull(r.TryFind(@"src\a.cs"));
        Assert.Null(r.TryFind(@"src\b.txt"));
    }

    [Fact]
    public void IgnoreHiddenAndSystem()
    {
        var fs = Create();
        fs.AddFile($@"{L}\visible.txt", T0);
        fs.AddFile($@"{L}\hidden.txt", T0);
        fs.File.SetAttributes($@"{L}\hidden.txt", FileAttributes.Hidden);
        Assert.NotNull(fs.Compare().TryFind("hidden.txt"));
        var r = fs.Compare(new CompareOptions { IgnoreHiddenAndSystem = true });
        Assert.Null(r.TryFind("hidden.txt"));
        Assert.NotNull(r.TryFind("visible.txt"));
    }

    [Fact]
    public void NonRecursive_OnlyTopLevel()
    {
        var fs = Create();
        fs.AddFile($@"{L}\top.txt", T0);
        fs.AddFile($@"{L}\sub\deep.txt", T0);
        var r = fs.Compare(new CompareOptions { Recursive = false });
        Assert.NotNull(r.TryFind("top.txt"));
        Assert.Empty(r.Find("sub").Children);
    }

    [Fact]
    public void SymlinkLoop_IsDetectedWhenFollowing()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a\file.txt", T0);
        fs.Directory.CreateSymbolicLink($@"{L}\a\loop", $@"{L}\a");

        var r = fs.Compare(new CompareOptions { SymlinkMode = SymlinkMode.Follow });
        var loop = r.Find(@"a\loop");
        Assert.Equal(DiffStatus.Error, loop.Status);
        Assert.Contains("loop", loop.ErrorMessage, StringComparison.OrdinalIgnoreCase);
        Assert.Empty(loop.Children);
    }

    [Fact]
    public void Symlinks_SkippedByDefault()
    {
        var fs = Create();
        fs.AddFile($@"{L}\target\file.txt", T0);
        fs.Directory.CreateSymbolicLink($@"{L}\link", $@"{L}\target");
        var r = fs.Compare();
        Assert.Null(r.TryFind("link"));
        Assert.NotNull(r.TryFind(@"target\file.txt"));
    }

    [Fact]
    public void Scan_DoesNotModifyFileSystem()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a\b.txt", T0);
        fs.AddFile($@"{R}\c.txt", T0);
        var before = fs.AllPaths.OrderBy(p => p).ToList();
        fs.Compare();
        Assert.Equal(before, fs.AllPaths.OrderBy(p => p).ToList());
    }

    [Fact]
    public async Task Scan_CanBeCancelled()
    {
        var fs = Create();
        for (int i = 0; i < 50; i++) fs.AddFile($@"{L}\d{i}\f.txt", T0);
        using var cts = new CancellationTokenSource();
        cts.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            new DiffEngine(fs).CompareAsync(L, R, new CompareOptions(), null, cts.Token));
    }

    [Fact]
    public async Task Scan_ReportsProgress()
    {
        var fs = Create();
        for (int i = 0; i < 10; i++) fs.AddFile($@"{L}\f{i}.txt", T0);
        var reports = new List<ScanProgress>();
        var progress = new SyncProgress<ScanProgress>(reports.Add);
        await new DiffEngine(fs).CompareAsync(L, R, new CompareOptions(), progress);
        Assert.Equal(10, reports.Last().ItemsScanned);
    }

    [Fact]
    public async Task RejectsSameOrNestedRoots()
    {
        var fs = Create();
        fs.Directory.CreateDirectory($@"{L}\inner");
        var engine = new DiffEngine(fs);
        await Assert.ThrowsAsync<InvalidOperationException>(() => engine.CompareAsync(L, L + "\\", new CompareOptions()));
        await Assert.ThrowsAsync<InvalidOperationException>(() => engine.CompareAsync(L, $@"{L}\inner", new CompareOptions()));
    }

    [Fact]
    public void Refresh_UpdatesNodeAndAncestors()
    {
        var fs = Create();
        fs.AddFile($@"{L}\d\a.txt", T0.AddHours(1));
        fs.AddFile($@"{R}\d\a.txt", T0);
        var r = fs.Compare();
        var node = r.Find(@"d\a.txt");
        Assert.Equal(DiffStatus.Differs, r.Root.Status);

        fs.File.SetLastWriteTimeUtc($@"{R}\d\a.txt", T0.AddHours(1));
        var engine = new DiffEngine(fs);
        DiffEngine.ApplyRefresh(engine.PrepareRefresh(r, node.Parent!, node.Name));

        Assert.Equal(DiffStatus.Identical, r.Find(@"d\a.txt").Status);
        Assert.Equal(DiffStatus.Identical, r.Find("d").Status);
        Assert.Equal(DiffStatus.Identical, r.Root.Status);
    }

    [Fact]
    public void Refresh_RemovesDeletedNode()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0);
        var r = fs.Compare();
        fs.File.Delete($@"{L}\a.txt");
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
