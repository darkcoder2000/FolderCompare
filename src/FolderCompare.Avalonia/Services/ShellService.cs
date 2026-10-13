using System.Diagnostics;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Input.Platform;

namespace FolderCompare.App.Services;

/// <summary>Opens files with the OS and reveals them in the file manager. One subclass per OS.</summary>
public abstract class ShellService : IShellService
{
    public static IShellService ForCurrentOS() =>
        OperatingSystem.IsWindows() ? new WindowsShellService()
        : OperatingSystem.IsMacOS() ? new MacShellService()
        : new LinuxShellService();

    public abstract string FileManagerName { get; }
    public virtual string TrashName => "Trash";
    public abstract void Open(string path);
    public abstract void ShowInFileManager(string path);

    public async Task SetClipboardTextAsync(string text)
    {
        var clipboard = TopLevel.GetTopLevel(ActiveWindow())?.Clipboard
                        ?? throw new InvalidOperationException("The clipboard is not available.");
        await clipboard.SetTextAsync(text);
    }

    internal static Window? ActiveWindow() =>
        Application.Current?.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop
            ? desktop.Windows.FirstOrDefault(w => w.IsActive) ?? desktop.MainWindow
            : null;

    /// <summary>Starts a program with arguments passed as a list (no shell, no quoting problems).</summary>
    protected static void Run(string program, params string[] args)
    {
        var info = new ProcessStartInfo(program) { UseShellExecute = false };
        foreach (var a in args) info.ArgumentList.Add(a);
        using var _ = Process.Start(info);
    }
}

public sealed class WindowsShellService : ShellService
{
    public override string FileManagerName => "Explorer";
    public override string TrashName => "Recycle Bin";

    public override void Open(string path) => Process.Start(new ProcessStartInfo(path) { UseShellExecute = true })?.Dispose();

    public override void ShowInFileManager(string path) =>
        Process.Start(new ProcessStartInfo("explorer.exe", $"/select,\"{path}\"") { UseShellExecute = true })?.Dispose();
}

public sealed class MacShellService : ShellService
{
    public override string FileManagerName => "Finder";

    public override void Open(string path) => Run("open", path);

    public override void ShowInFileManager(string path) => Run("open", "-R", path);
}

public sealed class LinuxShellService : ShellService
{
    public override string FileManagerName => "File Manager";

    public override void Open(string path) => Run("xdg-open", path);

    /// <summary>
    /// Asks the desktop's file manager (Nautilus, Dolphin, Nemo, ...) to select the item through the freedesktop
    /// FileManager1 D-Bus interface; without one, opens the containing folder.
    /// </summary>
    public override void ShowInFileManager(string path)
    {
        var uri = new Uri(path).AbsoluteUri;
        try
        {
            using var p = Process.Start(new ProcessStartInfo("gdbus")
            {
                UseShellExecute = false,
                RedirectStandardError = true,
                RedirectStandardOutput = true,
                ArgumentList =
                {
                    "call", "--session", "--dest", "org.freedesktop.FileManager1",
                    "--object-path", "/org/freedesktop/FileManager1",
                    "--method", "org.freedesktop.FileManager1.ShowItems", $"['{uri.Replace("'", "%27")}']", "",
                },
            });
            if (p is not null && p.WaitForExit(3000) && p.ExitCode == 0) return;
        }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException)
        {
            // gdbus is not installed; fall back below.
        }
        Run("xdg-open", Path.GetDirectoryName(path) ?? path);
    }
}
