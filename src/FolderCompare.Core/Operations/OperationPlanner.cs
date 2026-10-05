using FolderCompare.Core.Comparison;
using FolderCompare.Core.Models;

namespace FolderCompare.Core.Operations;

/// <summary>Turns a selection of nodes into a flat list of actions for preview and execution.</summary>
public static class OperationPlanner
{
    /// <summary>Removes nodes that are already covered by a selected ancestor, keeping the original order.</summary>
    public static List<DiffNode> TopLevelOnly(IEnumerable<DiffNode> nodes)
    {
        var list = nodes.Distinct().ToList();
        var set = new HashSet<DiffNode>(list);
        return list.Where(n => !Ancestors(n).Any(set.Contains)).ToList();
    }

    public static OperationPlan PlanCopy(DiffResult result, IEnumerable<DiffNode> nodes, CopyDirection direction)
    {
        var source = direction == CopyDirection.LeftToRight ? Side.Left : Side.Right;
        var target = direction == CopyDirection.LeftToRight ? Side.Right : Side.Left;
        var actions = new List<PlannedAction>();
        var notes = new List<string>();

        foreach (var node in TopLevelOnly(nodes))
            AddCopy(result, node, source, target, actions, notes, topLevel: true);

        return new OperationPlan(actions, notes);
    }

    public static OperationPlan PlanDelete(DiffResult result, IEnumerable<DiffNode> nodes, Sides sides)
    {
        var actions = new List<PlannedAction>();
        var notes = new List<string>();
        foreach (var node in TopLevelOnly(nodes))
        {
            bool any = false;
            foreach (var side in new[] { Side.Left, Side.Right })
            {
                if (!sides.HasFlag(side == Side.Left ? Sides.Left : Sides.Right)) continue;
                var snap = node.Get(side);
                if (snap is null) continue;
                any = true;
                actions.Add(new PlannedAction
                {
                    Kind = ActionKind.Delete,
                    Node = node,
                    TargetSide = side,
                    TargetPath = Path.Join(result.RootFor(side), node.RelativePath),
                    IsDirectory = snap.IsDirectory,
                    Size = side == Side.Left ? node.LeftTotalSize : node.RightTotalSize,
                    TargetTimeUtc = snap.LastWriteUtc,
                    TargetExists = true,
                });
            }
            if (!any) notes.Add($"{node.RelativePath}: does not exist on the chosen side.");
        }
        return new OperationPlan(actions, notes);
    }

    private static void AddCopy(DiffResult result, DiffNode node, Side source, Side target,
                                List<PlannedAction> actions, List<string> notes, bool topLevel)
    {
        var src = node.Get(source);
        var dst = node.Get(target);
        if (src is null)
        {
            if (topLevel) notes.Add($"{node.RelativePath}: does not exist on the source side.");
            return;
        }
        if (node.Status == DiffStatus.TypeConflict)
        {
            notes.Add($"{node.RelativePath}: skipped, file on one side and folder on the other.");
            return;
        }
        if (node.Status == DiffStatus.Error)
        {
            notes.Add($"{node.RelativePath}: skipped, {node.ErrorMessage}");
            return;
        }

        var targetPath = Path.Join(result.RootFor(target), node.RelativePath);
        if (node.IsDirectory)
        {
            if (dst is null)
            {
                actions.Add(new PlannedAction
                {
                    Kind = ActionKind.CreateDirectory,
                    Node = node,
                    TargetSide = target,
                    SourcePath = src.FullPath,
                    TargetPath = targetPath,
                    IsDirectory = true,
                });
            }
            foreach (var child in node.Children)
                AddCopy(result, child, source, target, actions, notes, topLevel: false);
            return;
        }

        if (node.Status == DiffStatus.Identical)
        {
            if (topLevel) notes.Add($"{node.RelativePath}: already identical.");
            return;
        }

        actions.Add(new PlannedAction
        {
            Kind = ActionKind.CopyFile,
            Node = node,
            TargetSide = target,
            SourcePath = src.FullPath,
            TargetPath = targetPath,
            Size = src.Size,
            SourceTimeUtc = src.LastWriteUtc,
            TargetTimeUtc = dst?.LastWriteUtc,
            TargetExists = dst is not null,
            TargetIsNewer = dst is not null && TimestampComparer.IsNewer(dst.LastWriteUtc, src.LastWriteUtc, result.Options.Tolerance),
        });
    }

    private static IEnumerable<DiffNode> Ancestors(DiffNode node)
    {
        for (var p = node.Parent; p is not null; p = p.Parent) yield return p;
    }
}
