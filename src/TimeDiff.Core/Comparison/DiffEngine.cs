using System.IO.Abstractions;
using TimeDiff.Core.Models;
using TimeDiff.Core.Operations;

namespace TimeDiff.Core.Comparison;

/// <summary>Data gathered off the UI thread for refreshing one child of a folder node.</summary>
public sealed class RefreshData
{
    internal RefreshData(DiffNode parent, string name, DiffNode? fresh, List<(DiffNode Node, FileInfoSnapshot? Left, FileInfoSnapshot? Right)> ancestors)
    {
        Parent = parent;
        Name = name;
        Fresh = fresh;
        Ancestors = ancestors;
    }

    public DiffNode Parent { get; }
    public string Name { get; }
    /// <summary>The rescanned node, or null if the path no longer exists on either side.</summary>
    public DiffNode? Fresh { get; }
    internal List<(DiffNode Node, FileInfoSnapshot? Left, FileInfoSnapshot? Right)> Ancestors { get; }
}

/// <summary>Scans both trees and merges them into a <see cref="DiffNode"/> tree. Never modifies the file system.</summary>
public sealed class DiffEngine
{
    private readonly IFileSystem _fs;
    private readonly DirectoryScanner _scanner;

    public DiffEngine(IFileSystem fs)
    {
        _fs = fs;
        _scanner = new DirectoryScanner(fs);
    }

    public async Task<DiffResult> CompareAsync(string leftRoot, string rightRoot, CompareOptions options,
                                               IProgress<ScanProgress>? progress = null, CancellationToken ct = default)
    {
        var error = SafetyValidator.ValidateRoots(_fs, leftRoot, rightRoot);
        if (error is not null) throw new InvalidOperationException(error);

        leftRoot = SafetyValidator.Normalize(_fs, leftRoot);
        rightRoot = SafetyValidator.Normalize(_fs, rightRoot);
        var tracker = new ProgressTracker(progress);

        // Bounded parallelism: one scanner task per side.
        var leftTask = Task.Run(() => _scanner.ScanRoot(leftRoot, options, tracker, ct), ct);
        var rightTask = Task.Run(() => _scanner.ScanRoot(rightRoot, options, tracker, ct), ct);
        await Task.WhenAll(leftTask, rightTask).ConfigureAwait(false);

        var root = await Task.Run(() => Merge("", "", leftTask.Result, rightTask.Result, options), ct).ConfigureAwait(false);
        tracker.ReportFinal();
        return new DiffResult(leftRoot, rightRoot, options, root);
    }

    /// <summary>Rescans the child <paramref name="name"/> of <paramref name="parent"/> on both sides (I/O, call off the UI thread).</summary>
    public RefreshData PrepareRefresh(DiffResult result, DiffNode parent, string name, CancellationToken ct = default)
    {
        var rel = parent.RelativePath.Length == 0 ? name : parent.RelativePath + "\\" + name;
        var left = _scanner.ScanPath(result.LeftRoot, rel, result.Options, ct);
        var right = _scanner.ScanPath(result.RightRoot, rel, result.Options, ct);
        var fresh = left is null && right is null ? null : Merge(left?.Name ?? right!.Name, rel, left, right, result.Options);

        var ancestors = new List<(DiffNode, FileInfoSnapshot?, FileInfoSnapshot?)>();
        for (var a = parent; a is not null; a = a.Parent)
        {
            ancestors.Add((a, _scanner.Stat(_fs.Path.Join(result.LeftRoot, a.RelativePath)),
                              _scanner.Stat(_fs.Path.Join(result.RightRoot, a.RelativePath))));
        }
        return new RefreshData(parent, name, fresh, ancestors);
    }

    /// <summary>Applies a prepared refresh to the tree (call on the thread that owns the tree).</summary>
    public static void ApplyRefresh(RefreshData data)
    {
        var parent = data.Parent;
        int index = parent.Children.FindIndex(c => string.Equals(c.Name, data.Name, StringComparison.OrdinalIgnoreCase));
        if (data.Fresh is null)
        {
            if (index >= 0) parent.Children.RemoveAt(index);
        }
        else
        {
            data.Fresh.Parent = parent;
            if (index >= 0) parent.Children[index] = data.Fresh;
            else parent.Children.Add(data.Fresh);
        }

        foreach (var (node, left, right) in data.Ancestors)
        {
            if (node.Parent is not null)
            {
                node.Left = left;
                node.Right = right;
            }
            RollUp(node);
        }
    }

    internal static DiffNode Merge(string name, string rel, ScanEntry? left, ScanEntry? right, CompareOptions options)
    {
        bool leftDir = left?.IsDirectory ?? false;
        bool rightDir = right?.IsDirectory ?? false;

        if (left is not null && right is not null && leftDir != rightDir)
        {
            return new DiffNode(rel, name, false)
            {
                Left = left.Snapshot,
                Right = right.Snapshot,
                Status = DiffStatus.TypeConflict,
                ErrorMessage = leftDir ? "Folder on the left, file on the right." : "File on the left, folder on the right.",
                LeftTotalSize = left.Snapshot.Size,
                RightTotalSize = right.Snapshot.Size,
            };
        }

        bool isDir = leftDir || rightDir;
        var node = new DiffNode(rel, name, isDir) { Left = left?.Snapshot, Right = right?.Snapshot };

        var error = left?.Error is not null ? "Left: " + left.Error : right?.Error is not null ? "Right: " + right.Error : null;
        if (error is not null)
        {
            node.Status = DiffStatus.Error;
            node.ErrorMessage = error;
            return node;
        }

        if (!isDir)
        {
            node.LeftTotalSize = left?.Snapshot.Size ?? 0;
            node.RightTotalSize = right?.Snapshot.Size ?? 0;
            node.Status = left is null ? DiffStatus.OnlyRight
                : right is null ? DiffStatus.OnlyLeft
                : TimestampComparer.Compare(left.Snapshot.LastWriteUtc, right.Snapshot.LastWriteUtc, options.Tolerance, options.IgnoreDstOffset);
            return node;
        }

        if (left?.Children is not null)
        {
            foreach (var (childName, l) in left.Children)
            {
                ScanEntry? r = null;
                right?.Children?.TryGetValue(childName, out r);
                AddChild(node, Merge(l.Name, Join(rel, l.Name), l, r, options));
            }
        }
        if (right?.Children is not null)
        {
            foreach (var (childName, r) in right.Children)
            {
                if (left?.Children?.ContainsKey(childName) == true) continue;
                AddChild(node, Merge(r.Name, Join(rel, r.Name), null, r, options));
            }
        }

        RollUp(node);
        return node;
    }

    /// <summary>Recomputes a folder's sizes and rolled-up status from its children.</summary>
    public static void RollUp(DiffNode node)
    {
        if (!node.IsDirectory) return;
        long leftSize = 0, rightSize = 0;
        bool allIdentical = true;
        foreach (var c in node.Children)
        {
            leftSize += c.LeftTotalSize;
            rightSize += c.RightTotalSize;
            if (c.Status != DiffStatus.Identical) allIdentical = false;
        }
        node.LeftTotalSize = node.Left is null ? 0 : leftSize;
        node.RightTotalSize = node.Right is null ? 0 : rightSize;

        if (node.ErrorMessage is not null) node.Status = DiffStatus.Error;
        else if (node.Left is null) node.Status = DiffStatus.OnlyRight;
        else if (node.Right is null) node.Status = DiffStatus.OnlyLeft;
        else node.Status = allIdentical ? DiffStatus.Identical : DiffStatus.Differs;
    }

    private static void AddChild(DiffNode parent, DiffNode child)
    {
        child.Parent = parent;
        parent.Children.Add(child);
    }

    private static string Join(string rel, string name) => rel.Length == 0 ? name : rel + "\\" + name;
}
