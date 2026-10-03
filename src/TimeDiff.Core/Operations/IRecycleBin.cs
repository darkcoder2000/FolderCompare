namespace TimeDiff.Core.Operations;

/// <summary>Sends items to the Windows Recycle Bin. Implemented in the app (needs the Windows desktop runtime).</summary>
public interface IRecycleBin
{
    void DeleteFile(string path);
    void DeleteDirectory(string path);
}
