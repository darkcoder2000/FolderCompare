using System.Diagnostics;
using System.IO;
using System.Windows;

namespace TimeDiff.App.Services;

public interface IShellService
{
    void Open(string path);
    void ShowInExplorer(string path);
    void SetClipboardText(string text);
}

public sealed class ShellService : IShellService
{
    public void Open(string path) => Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });

    public void ShowInExplorer(string path) =>
        Process.Start(new ProcessStartInfo("explorer.exe", $"/select,\"{path}\"") { UseShellExecute = true });

    public void SetClipboardText(string text) => Clipboard.SetText(text);

    public static string LogDirectory =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "TimeDiff", "logs");
}
