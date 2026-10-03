namespace TimeDiff.Core.Models;

public sealed record FileInfoSnapshot(long Size, DateTime LastWriteUtc, FileAttributes Attributes, string FullPath)
{
    public bool IsDirectory => (Attributes & FileAttributes.Directory) != 0;
}
