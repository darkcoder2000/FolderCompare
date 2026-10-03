using System.IO.Abstractions;
using System.IO.Abstractions.TestingHelpers;
using TimeDiff.Core.Comparison;
using TimeDiff.Core.Models;
using TimeDiff.Core.Operations;
using static TimeDiff.Core.Tests.TestFs;

namespace TimeDiff.Core.Tests;

public class FileOperationTests
{
    private static OperationOptions Options(OverwritePolicy policy = OverwritePolicy.Overwrite, bool recycle = false) =>
        new() { LeftRoot = L, RightRoot = R, OverwritePolicy = policy, DeleteToRecycleBin = recycle };

    private static OperationSummary Run(IFileSystem fs, OperationPlan plan, OperationOptions options,
                                        OverwriteResolver? resolver = null, FakeRecycleBin? bin = null, CancellationToken ct = default) =>
        new FileOperationExecutor(fs, bin ?? new FakeRecycleBin()).ExecuteAsync(plan.Actions, options, resolver, null, ct).GetAwaiter().GetResult();

    private static OperationPlan CopyPlan(DiffResult r, string rel, CopyDirection dir = CopyDirection.LeftToRight) =>
        OperationPlanner.PlanCopy(r, [r.Find(rel)], dir);

    [Fact]
    public void Copy_PreservesTimestampAndCreatesParents()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a\b\c.txt", T0, "hello");
        var r = fs.Compare();

        var summary = Run(fs, CopyPlan(r, "a"), Options());

        Assert.Equal(0, summary.Failed);
        var target = $@"{R}\a\b\c.txt";
        Assert.True(fs.File.Exists(target));
        Assert.Equal("hello", fs.File.ReadAllText(target));
        Assert.Equal(T0, fs.File.GetLastWriteTimeUtc(target));
        Assert.False(fs.File.Exists(target + FileOperationExecutor.TempSuffix));
    }

    [Fact]
    public void Copy_ThenRefresh_MakesIdentical()
    {
        var fs = Create();
        fs.AddFile($@"{L}\d\x.txt", T0.AddHours(2), "new");
        fs.AddFile($@"{R}\d\x.txt", T0, "old");
        var r = fs.Compare();
        var node = r.Find(@"d\x.txt");

        Run(fs, CopyPlan(r, @"d\x.txt"), Options());
        DiffEngine.ApplyRefresh(new DiffEngine(fs).PrepareRefresh(r, node.Parent!, node.Name));

        Assert.Equal(DiffStatus.Identical, r.Find(@"d\x.txt").Status);
        Assert.Equal(DiffStatus.Identical, r.Root.Status);
        Assert.Equal("new", fs.File.ReadAllText($@"{R}\d\x.txt"));
    }

    [Fact]
    public void Copy_RightToLeft()
    {
        var fs = Create();
        fs.AddFile($@"{R}\only.txt", T0, "r");
        var r = fs.Compare();
        Run(fs, CopyPlan(r, "only.txt", CopyDirection.RightToLeft), Options());
        Assert.Equal("r", fs.File.ReadAllText($@"{L}\only.txt"));
    }

    [Fact]
    public void Copy_PreservesReadOnlyAndOverwritesReadOnlyTarget()
    {
        var fs = Create();
        fs.AddFile($@"{L}\ro.txt", T0.AddHours(1), "new");
        fs.File.SetAttributes($@"{L}\ro.txt", FileAttributes.ReadOnly);
        fs.AddFile($@"{R}\ro.txt", T0, "old");
        fs.File.SetAttributes($@"{R}\ro.txt", FileAttributes.ReadOnly);
        var r = fs.Compare();

        var summary = Run(fs, CopyPlan(r, "ro.txt"), Options());

        Assert.Equal(1, summary.Succeeded);
        Assert.Equal("new", fs.File.ReadAllText($@"{R}\ro.txt"));
        Assert.True(fs.File.GetAttributes($@"{R}\ro.txt").HasFlag(FileAttributes.ReadOnly));
    }

    [Fact]
    public void Copy_IdenticalFilesInsideFolderAreNotCopied()
    {
        var fs = Create();
        fs.AddFile($@"{L}\d\same.txt", T0);
        fs.AddFile($@"{R}\d\same.txt", T0);
        fs.AddFile($@"{L}\d\new.txt", T0);
        var plan = CopyPlan(fs.Compare(), "d");
        var action = Assert.Single(plan.Actions);
        Assert.EndsWith("new.txt", action.TargetPath);
    }

    [Fact]
    public void Overwrite_Skip()
    {
        var fs = ConflictFs(out var r);
        var summary = Run(fs, CopyPlan(r, "x.txt"), Options(OverwritePolicy.Skip));
        Assert.Equal(1, summary.Skipped);
        Assert.Equal("old", fs.File.ReadAllText($@"{R}\x.txt"));
    }

    [Fact]
    public void Overwrite_IfNewer_OverwritesOlderTarget()
    {
        var fs = ConflictFs(out var r);
        var summary = Run(fs, CopyPlan(r, "x.txt"), Options(OverwritePolicy.OverwriteIfNewer));
        Assert.Equal(1, summary.Succeeded);
        Assert.Equal("new", fs.File.ReadAllText($@"{R}\x.txt"));
    }

    [Fact]
    public void Overwrite_IfNewer_SkipsNewerTarget()
    {
        var fs = ConflictFs(out var r);
        var summary = Run(fs, CopyPlan(r, "x.txt", CopyDirection.RightToLeft), Options(OverwritePolicy.OverwriteIfNewer));
        Assert.Equal(1, summary.Skipped);
        Assert.Equal("new", fs.File.ReadAllText($@"{L}\x.txt"));
    }

    [Fact]
    public void Overwrite_KeepBoth()
    {
        var fs = ConflictFs(out var r);
        var summary = Run(fs, CopyPlan(r, "x.txt"), Options(OverwritePolicy.KeepBoth));
        Assert.Equal(1, summary.Succeeded);
        Assert.Equal("old", fs.File.ReadAllText($@"{R}\x.txt"));
        Assert.Equal("new", fs.File.ReadAllText($@"{R}\x (1).txt"));
    }

    [Fact]
    public void Overwrite_Ask_UsesResolverAndApplyToAll()
    {
        var fs = Create();
        for (int i = 0; i < 3; i++)
        {
            fs.AddFile($@"{L}\f{i}.txt", T0.AddHours(1), "new");
            fs.AddFile($@"{R}\f{i}.txt", T0, "old");
        }
        var r = fs.Compare();
        int asked = 0;
        OverwriteResolver resolver = (_, _) =>
        {
            asked++;
            return Task.FromResult(new OverwriteDecision(OverwriteChoice.Overwrite, ApplyToAll: true));
        };

        var summary = Run(fs, OperationPlanner.PlanCopy(r, r.Root.Children, CopyDirection.LeftToRight), Options(OverwritePolicy.Ask), resolver);

        Assert.Equal(1, asked);
        Assert.Equal(3, summary.Succeeded);
    }

    [Fact]
    public void Overwrite_NewerTarget_AlwaysAsksEvenWithOverwritePolicy()
    {
        var fs = ConflictFs(out var r);
        OverwriteConflict? seen = null;
        OverwriteResolver resolver = (c, _) =>
        {
            seen = c;
            return Task.FromResult(new OverwriteDecision(OverwriteChoice.Skip, false));
        };

        var summary = Run(fs, CopyPlan(r, "x.txt", CopyDirection.RightToLeft), Options(OverwritePolicy.Overwrite), resolver);

        Assert.NotNull(seen);
        Assert.True(seen!.TargetIsNewer);
        Assert.Equal(1, summary.Skipped);
        Assert.Equal("new", fs.File.ReadAllText($@"{L}\x.txt"));
    }

    [Fact]
    public void Overwrite_CancelFromResolverStopsQueue()
    {
        var fs = ConflictFs(out var r);
        OverwriteResolver resolver = (_, _) => Task.FromResult(new OverwriteDecision(OverwriteChoice.Cancel, false));
        var summary = Run(fs, CopyPlan(r, "x.txt"), Options(OverwritePolicy.Ask), resolver);
        Assert.True(summary.Cancelled);
        Assert.Equal("old", fs.File.ReadAllText($@"{R}\x.txt"));
    }

    [Fact]
    public void Verification_FailsWhenTimestampNotPreserved()
    {
        var mock = Create();
        mock.AddFile($@"{L}\a.txt", T0, "x");
        var r = mock.Compare();
        var fs = new NoTimestampFileSystem(mock);

        var summary = Run(fs, CopyPlan(r, "a.txt"), Options());

        var item = Assert.Single(summary.Items);
        Assert.Equal(ItemOutcome.Failed, item.Outcome);
        Assert.Contains("timestamp", item.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void PathTraversal_IsRejected()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0);
        var r = fs.Compare();
        var evil = new PlannedAction
        {
            Kind = ActionKind.CopyFile,
            Node = r.Find("a.txt"),
            TargetSide = Side.Right,
            SourcePath = $@"{L}\a.txt",
            TargetPath = $@"{R}\..\outside\a.txt",
            Size = 4,
        };
        var summary = new FileOperationExecutor(fs, new FakeRecycleBin())
            .ExecuteAsync([evil], Options(), null).GetAwaiter().GetResult();

        Assert.Equal(ItemOutcome.Failed, summary.Items[0].Outcome);
        Assert.False(fs.File.Exists(@"C:\outside\a.txt"));
    }

    [Fact]
    public void DeleteOfRoot_IsRejected()
    {
        var fs = Create();
        var r = fs.Compare();
        var action = new PlannedAction { Kind = ActionKind.Delete, Node = r.Root, TargetSide = Side.Left, TargetPath = L, IsDirectory = true };
        var summary = new FileOperationExecutor(fs, new FakeRecycleBin()).ExecuteAsync([action], Options(), null).GetAwaiter().GetResult();
        Assert.Equal(ItemOutcome.Failed, summary.Items[0].Outcome);
        Assert.True(fs.Directory.Exists(L));
    }

    [Fact]
    public void Delete_Permanent()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0);
        fs.AddFile($@"{L}\d\b.txt", T0);
        fs.File.SetAttributes($@"{L}\d\b.txt", FileAttributes.ReadOnly);
        fs.AddFile($@"{R}\a.txt", T0);
        var r = fs.Compare();

        var summary = Run(fs, OperationPlanner.PlanDelete(r, [r.Find("a.txt"), r.Find("d")], Sides.Left), Options());

        Assert.Equal(2, summary.Succeeded);
        Assert.False(fs.File.Exists($@"{L}\a.txt"));
        Assert.False(fs.Directory.Exists($@"{L}\d"));
        Assert.True(fs.File.Exists($@"{R}\a.txt"));
    }

    [Fact]
    public void Delete_ToRecycleBin_UsesRecycleBin()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0);
        fs.AddFile($@"{R}\a.txt", T0);
        var r = fs.Compare();
        var bin = new FakeRecycleBin();

        Run(fs, OperationPlanner.PlanDelete(r, [r.Find("a.txt")], Sides.Both), Options(recycle: true), bin: bin);

        Assert.Equal([$@"{L}\a.txt", $@"{R}\a.txt"], bin.Files);
        Assert.True(fs.File.Exists($@"{L}\a.txt")); // fake bin does not touch the file system
    }

    [Fact]
    public void PlanDelete_SkipsAncestorDuplicates()
    {
        var fs = Create();
        fs.AddFile($@"{L}\d\a.txt", T0);
        var r = fs.Compare();
        var plan = OperationPlanner.PlanDelete(r, [r.Find(@"d\a.txt"), r.Find("d")], Sides.Left);
        Assert.Single(plan.Actions);
        Assert.True(plan.Actions[0].IsDirectory);
    }

    [Fact]
    public void Copy_PreCancelled_DoesNothing()
    {
        var fs = Create();
        fs.AddFile($@"{L}\a.txt", T0);
        var r = fs.Compare();
        using var cts = new CancellationTokenSource();
        cts.Cancel();
        var summary = Run(fs, CopyPlan(r, "a.txt"), Options(), ct: cts.Token);
        Assert.True(summary.Cancelled);
        Assert.False(fs.File.Exists($@"{R}\a.txt"));
    }

    [Fact]
    public async Task Copy_CancelMidFile_LeavesNoPartialFile()
    {
        var fs = Create();
        fs.AddFile($@"{L}\big.bin", new MockFileData(new byte[40 * 1024 * 1024]));
        fs.File.SetLastWriteTimeUtc($@"{L}\big.bin", T0);
        var r = fs.Compare();
        using var cts = new CancellationTokenSource();
        var progress = new InlineProgress(p =>
        {
            if (p.FileBytesDone > 0 && p.FileBytesDone < p.FileBytesTotal) cts.Cancel();
        });

        var summary = await new FileOperationExecutor(fs, new FakeRecycleBin())
            .ExecuteAsync(CopyPlan(r, "big.bin").Actions, Options(), null, progress, cts.Token);

        Assert.True(summary.Cancelled);
        Assert.False(fs.File.Exists($@"{R}\big.bin"));
        Assert.False(fs.File.Exists($@"{R}\big.bin" + FileOperationExecutor.TempSuffix));
    }

    [Fact]
    public void Rename_RenamesWithinFolder()
    {
        var fs = Create();
        fs.AddFile($@"{L}\d\a.txt", T0);
        var ex = new FileOperationExecutor(fs, new FakeRecycleBin());
        ex.Rename(Options(), Side.Left, @"d\a.txt", "b.txt");
        Assert.True(fs.File.Exists($@"{L}\d\b.txt"));
        Assert.Throws<ArgumentException>(() => ex.Rename(Options(), Side.Left, @"d\b.txt", @"..\..\evil.txt"));
    }

    private static MockFileSystem ConflictFs(out DiffResult result)
    {
        var fs = Create();
        fs.AddFile($@"{L}\x.txt", T0.AddHours(1), "new");
        fs.AddFile($@"{R}\x.txt", T0, "old");
        result = fs.Compare();
        return fs;
    }

    private sealed class InlineProgress(Action<OperationProgress> handler) : IProgress<OperationProgress>
    {
        public void Report(OperationProgress value) => handler(value);
    }
}

internal sealed class FakeRecycleBin : IRecycleBin
{
    public List<string> Files { get; } = new();
    public List<string> Directories { get; } = new();
    public void DeleteFile(string path) => Files.Add(path);
    public void DeleteDirectory(string path) => Directories.Add(path);
}

/// <summary>A file system whose File.SetLastWriteTimeUtc silently does nothing, to exercise copy verification.</summary>
internal sealed class NoTimestampFileSystem(MockFileSystem inner) : IFileSystem
{
    public IFile File { get; } = new NoTimestampFile(inner);
    public IDirectory Directory => inner.Directory;
    public IFileInfoFactory FileInfo => inner.FileInfo;
    public IFileStreamFactory FileStream => inner.FileStream;
    public IPath Path => inner.Path;
    public IDirectoryInfoFactory DirectoryInfo => inner.DirectoryInfo;
    public IDriveInfoFactory DriveInfo => inner.DriveInfo;
    public IFileSystemWatcherFactory FileSystemWatcher => inner.FileSystemWatcher;

    private sealed class NoTimestampFile(MockFileSystem fs) : MockFile(fs)
    {
        public override void SetLastWriteTimeUtc(string path, DateTime lastWriteTimeUtc) { }
    }
}
