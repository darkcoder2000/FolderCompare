using FolderCompare.Core.Models;

namespace FolderCompare.Core.Comparison;

public static class TimestampComparer
{
    private static readonly TimeSpan OneHour = TimeSpan.FromHours(1);

    public static DiffStatus Compare(DateTime leftUtc, DateTime rightUtc, TimeSpan tolerance, bool ignoreDstOffset)
    {
        var diff = (leftUtc - rightUtc).Duration();
        if (diff <= tolerance) return DiffStatus.Identical;
        if (ignoreDstOffset && (diff - OneHour).Duration() <= tolerance) return DiffStatus.Identical;
        return leftUtc > rightUtc ? DiffStatus.NewerLeft : DiffStatus.NewerRight;
    }

    /// <summary>True when <paramref name="a"/> is newer than <paramref name="b"/> by more than the tolerance.</summary>
    public static bool IsNewer(DateTime a, DateTime b, TimeSpan tolerance) => a - b > tolerance;
}
