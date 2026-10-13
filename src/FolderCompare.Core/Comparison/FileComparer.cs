using FolderCompare.Core.Models;

namespace FolderCompare.Core.Comparison;

public readonly record struct FileComparison(DiffStatus Status, string? ErrorMessage = null);

/// <summary>
/// Decides the status of a file that exists on both sides: size first (always), then timestamp, then optionally
/// the content. Never modifies the file system.
/// </summary>
public static class FileComparer
{
    /// <summary>True when <see cref="Compare"/> would read the file contents for this pair.</summary>
    public static bool NeedsContentCheck(FileInfoSnapshot left, FileInfoSnapshot right, CompareOptions options)
    {
        if (left.Size != right.Size || options.ContentCompare == ContentCompareMode.Off) return false;
        bool sameTime = TimeStatus(left, right, options) == DiffStatus.Identical;
        return sameTime || options.ContentCompare == ContentCompareMode.WhenSizesEqual;
    }

    /// <summary>
    /// Compares two existing files. With <paramref name="content"/> null the content is never read (size and
    /// timestamp only). Read errors produce <see cref="DiffStatus.Error"/>; cancellation throws.
    /// </summary>
    public static FileComparison Compare(FileInfoSnapshot left, FileInfoSnapshot right, CompareOptions options,
                                         IContentComparer? content, CancellationToken ct, Action<long>? bytesRead = null)
    {
        var time = TimeStatus(left, right, options);
        bool sameTime = time == DiffStatus.Identical;

        if (left.Size != right.Size)
            return new(sameTime ? DiffStatus.SameTimeDifferentContent : time);

        if (content is null || !NeedsContentCheck(left, right, options))
            return new(time);

        bool equal;
        try
        {
            equal = content.AreEqual(left.FullPath, right.FullPath, bytesRead, ct);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return new(DiffStatus.Error, "Content compare failed: " + ex.Message);
        }

        if (sameTime) return new(equal ? DiffStatus.Identical : DiffStatus.SameTimeDifferentContent);
        return new(equal ? DiffStatus.SameContentDifferentTime : time);
    }

    private static DiffStatus TimeStatus(FileInfoSnapshot left, FileInfoSnapshot right, CompareOptions options) =>
        TimestampComparer.Compare(left.LastWriteUtc, right.LastWriteUtc, options.Tolerance, options.IgnoreDstOffset);
}
