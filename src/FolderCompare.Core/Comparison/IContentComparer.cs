namespace FolderCompare.Core.Comparison;

/// <summary>Pairwise byte comparison of two files. Implementations must never write.</summary>
public interface IContentComparer
{
    /// <summary>
    /// Returns true when both files have the same bytes. <paramref name="bytesRead"/> receives the number of bytes
    /// compared per chunk (per pair, not per side). Throws <see cref="IOException"/> or
    /// <see cref="UnauthorizedAccessException"/> on read errors and <see cref="OperationCanceledException"/> on cancel.
    /// </summary>
    bool AreEqual(string leftPath, string rightPath, Action<long>? bytesRead, CancellationToken ct);
}
