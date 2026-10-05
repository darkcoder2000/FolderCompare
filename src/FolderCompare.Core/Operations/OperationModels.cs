using FolderCompare.Core.Models;

namespace FolderCompare.Core.Operations;

public enum CopyDirection
{
    LeftToRight,
    RightToLeft,
}

[Flags]
public enum Sides
{
    None = 0,
    Left = 1,
    Right = 2,
    Both = Left | Right,
}

public enum OverwritePolicy
{
    Ask,
    Skip,
    Overwrite,
    OverwriteIfNewer,
    KeepBoth,
}

public enum ActionKind
{
    CopyFile,
    CreateDirectory,
    Delete,
}

public sealed class PlannedAction
{
    public required ActionKind Kind { get; init; }
    public required DiffNode Node { get; init; }
    /// <summary>The side that is written to (copy destination or delete side).</summary>
    public required Side TargetSide { get; init; }
    public string? SourcePath { get; init; }
    public required string TargetPath { get; init; }
    public bool IsDirectory { get; init; }
    public long Size { get; init; }
    public DateTime? SourceTimeUtc { get; init; }
    public DateTime? TargetTimeUtc { get; init; }
    public bool TargetExists { get; init; }
    /// <summary>The existing destination is newer than the source (overwriting it may be a mistake).</summary>
    public bool TargetIsNewer { get; init; }

    public string Description => Kind switch
    {
        ActionKind.CopyFile => $"Copy {SourcePath} -> {TargetPath}",
        ActionKind.CreateDirectory => $"Create folder {TargetPath}",
        _ => $"Delete {TargetPath}",
    };
}

public sealed record OperationPlan(IReadOnlyList<PlannedAction> Actions, IReadOnlyList<string> Notes)
{
    public long TotalBytes => Actions.Sum(a => a.Size);
}

public sealed record OperationOptions
{
    public required string LeftRoot { get; init; }
    public required string RightRoot { get; init; }
    public TimeSpan Tolerance { get; init; } = TimeSpan.FromSeconds(2);
    public OverwritePolicy OverwritePolicy { get; init; } = OverwritePolicy.Ask;
    public bool DeleteToRecycleBin { get; init; } = true;

    public string RootFor(Side side) => side == Side.Left ? LeftRoot : RightRoot;
}

public sealed record OverwriteConflict(
    string SourcePath, string TargetPath,
    long SourceSize, DateTime SourceTimeUtc,
    long TargetSize, DateTime TargetTimeUtc,
    bool TargetIsNewer);

public enum OverwriteChoice
{
    Skip,
    Overwrite,
    OverwriteIfNewer,
    KeepBoth,
    Cancel,
}

public sealed record OverwriteDecision(OverwriteChoice Choice, bool ApplyToAll);

public delegate Task<OverwriteDecision> OverwriteResolver(OverwriteConflict conflict, CancellationToken ct);

public sealed record OperationProgress(
    string CurrentItem,
    long FileBytesDone, long FileBytesTotal,
    long TotalBytesDone, long TotalBytes,
    int ItemsDone, int ItemsTotal);

public enum ItemOutcome
{
    Succeeded,
    Skipped,
    Failed,
}

public sealed record OperationItemResult(PlannedAction Action, ItemOutcome Outcome, string? Message, string? FinalTargetPath = null);

public sealed class OperationSummary
{
    public List<OperationItemResult> Items { get; } = new();
    public bool Cancelled { get; set; }

    public int Succeeded => Items.Count(i => i.Outcome == ItemOutcome.Succeeded);
    public int Skipped => Items.Count(i => i.Outcome == ItemOutcome.Skipped);
    public int Failed => Items.Count(i => i.Outcome == ItemOutcome.Failed);

    public string ToReport()
    {
        var sb = new System.Text.StringBuilder();
        sb.AppendLine($"Succeeded: {Succeeded}   Skipped: {Skipped}   Failed: {Failed}{(Cancelled ? "   (cancelled)" : "")}");
        foreach (var group in Items.Where(i => i.Outcome != ItemOutcome.Succeeded).GroupBy(i => i.Outcome))
        {
            sb.AppendLine();
            sb.AppendLine(group.Key + ":");
            foreach (var item in group)
                sb.AppendLine($"  {item.Action.Description}{(item.Message is null ? "" : " - " + item.Message)}");
        }
        return sb.ToString();
    }
}
