using System.Diagnostics;
using System.IO;
using System.Windows;

namespace FolderCompare.App.Services;

public sealed class ShellService : IShellService
{
    public void Open(string path) => Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });

    public void ShowInFileManager(string path) =>
        Process.Start(new ProcessStartInfo("explorer.exe", $"/select,\"{path}\"") { UseShellExecute = true });

    public Task SetClipboardTextAsync(string text)
    {
        Clipboard.SetText(text);
        return Task.CompletedTask;
    }

    public string FileManagerName => "Explorer";
    public string TrashName => "Recycle Bin";
}
