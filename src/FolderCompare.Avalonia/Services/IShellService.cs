using System.IO;

namespace FolderCompare.App.Services;

/// <summary>Opening files and folders with the OS, revealing them in the file manager, and the clipboard.</summary>
public interface IShellService
{
    void Open(string path);
    void ShowInFileManager(string path);
    Task SetClipboardTextAsync(string text);

    /// <summary>"Recycle Bin" on Windows, "Trash" elsewhere.</summary>
    string TrashName { get; }

    /// <summary>"Explorer", "Finder" or "File Manager", for menu texts.</summary>
    string FileManagerName { get; }
}

public static class AppPaths
{
    /// <summary>%LOCALAPPDATA%\FolderCompare\logs on Windows, ~/.local/share/FolderCompare/logs on Linux, ~/Library/Logs/FolderCompare on macOS.</summary>
    public static string LogDirectory => OperatingSystem.IsMacOS()
        ? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Library", "Logs", "FolderCompare")
        : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "FolderCompare", "logs");
}
