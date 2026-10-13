using FolderCompare.Core.TextDiff;

namespace FolderCompare.App.Services;

/// <summary>A file pair to open in the text compare window. A side that does not exist yet starts empty.</summary>
public sealed record TextCompareRequest(
    string LeftPath, bool LeftExists,
    string RightPath, bool RightExists,
    string Title,
    Action? FileSaved);

/// <summary>One side as loaded from disk.</summary>
public sealed record TextSide(string Path, bool Exists, LoadedText Text, DateTime? LastWriteUtc);

public interface ITextCompareLauncher
{
    Task OpenAsync(TextCompareRequest request);

    /// <summary>Asks every open compare window about unsaved changes. False when the user cancelled.</summary>
    Task<bool> ConfirmCloseAllAsync();
}
