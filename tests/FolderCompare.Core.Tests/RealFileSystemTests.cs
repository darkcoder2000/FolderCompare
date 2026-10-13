using System.IO.Abstractions;
using FolderCompare.Core.Comparison;
using FolderCompare.Core.Models;
using FolderCompare.Core.Operations;

namespace FolderCompare.Core.Tests;

/// <summary>Integration tests against the real file system of the host OS (long paths, locked files, Unix modes, case).</summary>
public sealed class RealFileSystemTests : IDisposable
{
    private static readonly DateTime T0 = new(2024, 5, 1, 12, 0, 0, DateTimeKind.Utc);
    private readonly string _root = Path.Combine(Path.GetTempPath(), "FolderCompareTests", Guid.NewGuid().ToString("N"));
    private readonly FileSystem _fs = new();

    private string Left => Path.Combine(_root, "left");
    private string Right => Path.Combine(_root, "right");

    public RealFileSystemTests()
    {
        Directory.CreateDirectory(Left);
        Directory.CreateDirectory(Right);
    }

    public void Dispose()
    {
        try
        {
            Directory.Delete(_root, recursive: true);
        }
        catch (IOException)
        {
        }
    }

    [Fact]
    public async Task LongPathsAndUnicode_AreScannedAndCopied()
    {
        var deep = string.Join(Path.DirectorySeparatorChar, Enumerable.Range(0, 12).Select(i => $"folder_{i}_" + new string('x', 20)));
        var rel = Path.Combine(deep, "Ünïcödé 文件.txt");
        var source = Path.Combine(Left, rel);
        Assert.True(source.Length > 260);
        Directory.CreateDirectory(Path.GetDirectoryName(source)!);
        File.WriteAllText(source, "long");
        File.SetLastWriteTimeUtc(source, T0);

        var result = await new DiffEngine(_fs).CompareAsync(Left, Right, new CompareOptions());
        var node = result.Root.Descendants().Single(n => n.Name == "Ünïcödé 文件.txt");
        Assert.Equal(DiffStatus.OnlyLeft, node.Status);

        var plan = OperationPlanner.PlanCopy(result, [result.Root.Children[0]], CopyDirection.LeftToRight);
        var summary = await new FileOperationExecutor(_fs, new FakeRecycleBin())
            .ExecuteAsync(plan.Actions, new OperationOptions { LeftRoot = result.LeftRoot, RightRoot = result.RightRoot }, null);

        Assert.Equal(0, summary.Failed);
        Assert.Equal(T0, File.GetLastWriteTimeUtc(Path.Combine(Right, rel)));
    }

    [Fact]
    public async Task LockedSource_IsReportedAsFailureAndQueueContinues()
    {
        var locked = Path.Combine(Left, "locked.txt");
        var free = Path.Combine(Left, "free.txt");
        File.WriteAllText(locked, "a");
        File.WriteAllText(free, "b");

        var result = await new DiffEngine(_fs).CompareAsync(Left, Right, new CompareOptions());
        var plan = OperationPlanner.PlanCopy(result, result.Root.Children, CopyDirection.LeftToRight);

        OperationSummary summary;
        using (new FileStream(locked, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
        {
            summary = await new FileOperationExecutor(_fs, new FakeRecycleBin())
                .ExecuteAsync(plan.Actions, new OperationOptions { LeftRoot = result.LeftRoot, RightRoot = result.RightRoot }, null);
        }

        Assert.Equal(1, summary.Failed);
        Assert.Equal(1, summary.Succeeded);
        Assert.False(File.Exists(Path.Combine(Right, "locked.txt")));
        Assert.False(File.Exists(Path.Combine(Right, "locked.txt" + FileOperationExecutor.TempSuffix)));
        Assert.True(File.Exists(Path.Combine(Right, "free.txt")));
    }

    [Fact]
    public async Task UnixFileMode_IsCopied()
    {
        if (OperatingSystem.IsWindows()) return;
        var source = Path.Combine(Left, "run.sh");
        File.WriteAllText(source, "#!/bin/sh");
        const UnixFileMode mode = UnixFileMode.UserRead | UnixFileMode.UserExecute | UnixFileMode.GroupRead | UnixFileMode.OtherRead;
        File.SetUnixFileMode(source, mode);

        var result = await new DiffEngine(_fs).CompareAsync(Left, Right, new CompareOptions());
        var plan = OperationPlanner.PlanCopy(result, result.Root.Children, CopyDirection.LeftToRight);
        var summary = await new FileOperationExecutor(_fs, new FakeRecycleBin())
            .ExecuteAsync(plan.Actions, new OperationOptions { LeftRoot = result.LeftRoot, RightRoot = result.RightRoot }, null);

        Assert.Equal(1, summary.Succeeded);
        Assert.Equal(mode, File.GetUnixFileMode(Path.Combine(Right, "run.sh")));
    }

    [Fact]
    public void FreedesktopTrash_OnLinux_MovesFileAndWritesInfo()
    {
        if (!OperatingSystem.IsLinux()) return;
        var file = Path.Combine(Left, "trash me.txt");
        File.WriteAllText(file, "bye");
        var homeTrash = Path.Combine(_root, "Trash");
        var trash = new FreedesktopTrash(_fs, homeTrash, "1000",
                                         p => FreedesktopTrash.MountPointOf(p, File.ReadAllLines("/proc/self/mountinfo")));

        Assert.True(trash.CanRecycle(file));
        trash.DeleteFile(file);

        Assert.False(File.Exists(file));
        Assert.Equal("bye", File.ReadAllText(Path.Combine(homeTrash, "files", "trash me.txt")));
        var info = File.ReadAllText(Path.Combine(homeTrash, "info", "trash me.txt.trashinfo"));
        Assert.Contains("Path=" + FreedesktopTrash.Encode(file), info);
    }

    [Fact]
    public async Task CaseOnlyNames_OnLinux_AreSeparateFiles()
    {
        if (!OperatingSystem.IsLinux()) return;
        File.WriteAllText(Path.Combine(Left, "a.txt"), "lower");
        File.WriteAllText(Path.Combine(Left, "A.txt"), "upper");

        var result = await new DiffEngine(_fs).CompareAsync(Left, Right, new CompareOptions());
        Assert.False(result.Options.Paths.IsIgnoreCase);
        Assert.Equal(2, result.Root.Children.Count);

        var plan = OperationPlanner.PlanCopy(result, result.Root.Children, CopyDirection.LeftToRight);
        var summary = await new FileOperationExecutor(_fs, new FakeRecycleBin())
            .ExecuteAsync(plan.Actions, new OperationOptions { LeftRoot = result.LeftRoot, RightRoot = result.RightRoot }, null);
        Assert.Equal(2, summary.Succeeded);
        Assert.Equal("upper", File.ReadAllText(Path.Combine(Right, "A.txt")));
        Assert.Equal("lower", File.ReadAllText(Path.Combine(Right, "a.txt")));
    }
}
