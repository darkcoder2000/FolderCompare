namespace FolderCompare.Core.Models;

/// <summary>
/// Scan progress. While file contents are being verified, <see cref="BytesTotal"/> is greater than zero and
/// <see cref="CurrentFolder"/> holds the file being compared.
/// </summary>
public sealed record ScanProgress(long ItemsScanned, string CurrentFolder, long BytesCompared = 0, long BytesTotal = 0);
