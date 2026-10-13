using System.Diagnostics;
using System.IO.Abstractions;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Microsoft.Extensions.Logging;
using Microsoft.VisualBasic.FileIO;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Services;

public static class RecycleBins
{
    /// <summary>The Recycle Bin on Windows, the Finder trash on macOS, the freedesktop.org trash elsewhere.</summary>
    public static IRecycleBin ForCurrentOS(IFileSystem fs, ILoggerFactory loggers)
    {
        if (OperatingSystem.IsWindows()) return new WindowsRecycleBin();
        if (OperatingSystem.IsMacOS()) return new MacTrash(loggers.CreateLogger<MacTrash>());
        return new FreedesktopTrash(fs, FreedesktopTrash.DefaultHomeTrash, LinuxUserId(),
                                    path => FreedesktopTrash.MountPointOf(path, ReadMountInfo()),
                                    logger: loggers.CreateLogger<FreedesktopTrash>());
    }

    private static string LinuxUserId() => getuid().ToString(System.Globalization.CultureInfo.InvariantCulture);

    private static string[] ReadMountInfo()
    {
        try
        {
            return File.ReadAllLines("/proc/self/mountinfo");
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return []; // everything counts as one file system: the home trash is used
        }
    }

    [DllImport("libc", SetLastError = false)]
    private static extern uint getuid();
}

[SupportedOSPlatform("windows")]
public sealed class WindowsRecycleBin : IRecycleBin
{
    public void DeleteFile(string path) =>
        Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(path, UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin, UICancelOption.ThrowException);

    public void DeleteDirectory(string path) =>
        Microsoft.VisualBasic.FileIO.FileSystem.DeleteDirectory(path, UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin, UICancelOption.ThrowException);
}

/// <summary>
/// Moves items to the macOS Trash with <c>-[NSFileManager trashItemAtURL:resultingItemURL:error:]</c> (the same as
/// Finder's "Move to Trash", so "Put Back" works), called through the Objective-C runtime.
/// </summary>
public sealed class MacTrash : IRecycleBin
{
    private const string ObjC = "/usr/lib/libobjc.A.dylib";
    private readonly ILogger _logger;

    public MacTrash(ILogger<MacTrash> logger) => _logger = logger;

    public void DeleteFile(string path) => Trash(path);
    public void DeleteDirectory(string path) => Trash(path);

    private void Trash(string path)
    {
        if (!File.Exists(path) && !Directory.Exists(path)) throw new FileNotFoundException("The item no longer exists.", path);
        IntPtr error = IntPtr.Zero;
        bool ok;
        try
        {
            var fileManager = Send(Class("NSFileManager"), Selector("defaultManager"));
            var nsPath = Send(Class("NSString"), Selector("stringWithUTF8String:"), path);
            var url = Send(Class("NSURL"), Selector("fileURLWithPath:"), nsPath);
            ok = SendBool(fileManager, Selector("trashItemAtURL:resultingItemURL:error:"), url, IntPtr.Zero, ref error);
        }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
        {
            _logger.LogWarning(ex, "Objective-C runtime not available, using Finder to move {Path} to the trash", path);
            TrashWithFinder(path);
            return;
        }
        if (!ok)
        {
            var description = error == IntPtr.Zero ? "unknown error" : ToString(Send(error, Selector("localizedDescription")));
            throw new IOException($"Could not move '{path}' to the Trash: {description}");
        }
    }

    private static void TrashWithFinder(string path)
    {
        var escaped = path.Replace("\\", "\\\\").Replace("\"", "\\\"");
        var info = new ProcessStartInfo("osascript") { UseShellExecute = false, RedirectStandardError = true };
        info.ArgumentList.Add("-e");
        info.ArgumentList.Add($"tell application \"Finder\" to delete POSIX file \"{escaped}\"");
        using var p = Process.Start(info) ?? throw new IOException("Could not start osascript.");
        var stderr = p.StandardError.ReadToEnd();
        p.WaitForExit();
        if (p.ExitCode != 0) throw new IOException($"Could not move '{path}' to the Trash: {stderr.Trim()}");
    }

    private static string ToString(IntPtr nsString) =>
        nsString == IntPtr.Zero ? "" : Marshal.PtrToStringUTF8(Send(nsString, Selector("UTF8String"))) ?? "";

    [DllImport(ObjC, EntryPoint = "objc_getClass")]
    private static extern IntPtr Class(string name);

    [DllImport(ObjC, EntryPoint = "sel_registerName")]
    private static extern IntPtr Selector(string name);

    [DllImport(ObjC, EntryPoint = "objc_msgSend")]
    private static extern IntPtr Send(IntPtr receiver, IntPtr selector);

    [DllImport(ObjC, EntryPoint = "objc_msgSend")]
    private static extern IntPtr Send(IntPtr receiver, IntPtr selector, IntPtr arg);

    [DllImport(ObjC, EntryPoint = "objc_msgSend")]
    private static extern IntPtr Send(IntPtr receiver, IntPtr selector, [MarshalAs(UnmanagedType.LPUTF8Str)] string arg);

    [DllImport(ObjC, EntryPoint = "objc_msgSend")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool SendBool(IntPtr receiver, IntPtr selector, IntPtr arg1, IntPtr arg2, ref IntPtr error);
}
