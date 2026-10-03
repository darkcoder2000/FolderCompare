using Microsoft.VisualBasic.FileIO;
using TimeDiff.Core.Operations;

namespace TimeDiff.App.Services;

public sealed class RecycleBin : IRecycleBin
{
    public void DeleteFile(string path) =>
        FileSystem.DeleteFile(path, UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin, UICancelOption.ThrowException);

    public void DeleteDirectory(string path) =>
        FileSystem.DeleteDirectory(path, UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin, UICancelOption.ThrowException);
}
