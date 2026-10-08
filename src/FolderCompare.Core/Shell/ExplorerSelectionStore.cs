using System.IO.Abstractions;

namespace FolderCompare.Core.Shell;

/// <summary>
/// Remembers the item picked with "Select as left" in the Explorer context menu until "Compare to left" is used.
/// Kept in its own file so a running instance saving settings.json cannot overwrite it.
/// </summary>
public sealed class ExplorerSelectionStore
{
    public const string FileName = "explorer-left.txt";

    private readonly IFileSystem _fs;
    private readonly string _directory;

    public ExplorerSelectionStore(IFileSystem fs, string directory)
    {
        _fs = fs;
        _directory = directory;
    }

    private string FilePath => _fs.Path.Combine(_directory, FileName);

    public void Save(string path)
    {
        _fs.Directory.CreateDirectory(_directory);
        _fs.File.WriteAllText(FilePath, path);
    }

    /// <summary>The stored left path, or null when none is stored.</summary>
    public string? Read()
    {
        if (!_fs.File.Exists(FilePath)) return null;
        var text = _fs.File.ReadAllText(FilePath).Trim();
        return text.Length == 0 ? null : text;
    }

    public void Clear()
    {
        if (_fs.File.Exists(FilePath)) _fs.File.Delete(FilePath);
    }
}
