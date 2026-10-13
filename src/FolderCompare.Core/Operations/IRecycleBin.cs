namespace FolderCompare.Core.Operations;

/// <summary>
/// Moves items to the Recycle Bin (Windows) or Trash (Linux, macOS) instead of deleting them.
/// Implementations: <see cref="FreedesktopTrash"/> (Linux) here, the Windows and macOS ones in the app.
/// </summary>
public interface IRecycleBin
{
    void DeleteFile(string path);
    void DeleteDirectory(string path);

    /// <summary>
    /// False when <paramref name="path"/> can't be moved to the trash (e.g. a Linux mount without a usable trash
    /// folder). The caller then asks before deleting permanently instead.
    /// </summary>
    bool CanRecycle(string path) => true;
}

/// <summary>Thrown when an item can't be moved to the trash; nothing has been deleted.</summary>
public sealed class TrashUnavailableException : IOException
{
    public TrashUnavailableException(string message) : base(message) { }
}
