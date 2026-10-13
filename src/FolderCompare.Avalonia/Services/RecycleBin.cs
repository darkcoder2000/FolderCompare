using System.Runtime.Versioning;
using Microsoft.VisualBasic.FileIO;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Services;

[SupportedOSPlatform("windows")]
public sealed class WindowsRecycleBin : IRecycleBin
{
    public void DeleteFile(string path) =>
        FileSystem.DeleteFile(path, UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin, UICancelOption.ThrowException);

    public void DeleteDirectory(string path) =>
        FileSystem.DeleteDirectory(path, UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin, UICancelOption.ThrowException);
}

/// <summary>Placeholder until the Linux and macOS trash implementations exist.</summary>
public sealed class UnavailableRecycleBin : IRecycleBin
{
    public void DeleteFile(string path) => throw new PlatformNotSupportedException("Moving to the trash is not supported on this system yet.");
    public void DeleteDirectory(string path) => DeleteFile(path);
}
