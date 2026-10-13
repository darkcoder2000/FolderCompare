using System.IO.Abstractions;

namespace FolderCompare.Core.Shell;

/// <summary>One registry value below HKCU. An empty <see cref="Name"/> is the key's default value.</summary>
public sealed record RegistryValue(string SubKey, string Name, string Value);

public enum PairKind
{
    BothFolders,
    BothFiles,
    Mixed,
    Missing,
}

/// <summary>
/// Registry layout of the Explorer context menu. The entries are <c>IExplorerCommand</c> handlers in
/// FolderCompare.ShellExt.dll, which decides visibility and titles itself and keeps the selected left item.
/// The CLSIDs and the <c>Software\FolderCompare</c> value names must match src/FolderCompare.ShellExt/ShellExt.cpp.
/// </summary>
public static class ExplorerMenuLayout
{
    public const string ClassesRoot = @"Software\Classes";

    /// <summary>Holds <see cref="ExePathValue"/>, read by the DLL to start the comparison.</summary>
    public const string AppKey = @"Software\FolderCompare";
    public const string ExePathValue = "ExePath";

    /// <summary>The item picked with "Select as left" (values Path, IsFolder). Written and cleared by the DLL.</summary>
    public const string LeftSelectionKey = @"Software\FolderCompare\LeftSelection";

    public const string CompareVerb = "FolderCompare.Compare";
    public const string SelectLeftVerb = "FolderCompare.SelectLeft";

    /// <summary>Verb of version 1.2.0 (static "Compare to left" entry), removed when registering.</summary>
    public const string LegacyCompareToLeftVerb = "FolderCompare.CompareToLeft";

    public const string CompareClsid = "{501F238C-E07D-479E-9123-448B4646A913}";
    public const string SelectLeftClsid = "{0E804BB4-FBFB-4028-B0C3-E348EF8E0F71}";

    public const string DllResourceName = "FolderCompare.ShellExt.dll";

    /// <summary>Files and folders.</summary>
    public static readonly IReadOnlyList<string> ClassKeys = new[] { "*", "Directory" };

    public static IReadOnlyList<string> Clsids { get; } = new[] { CompareClsid, SelectLeftClsid };

    /// <summary>All verb keys below <see cref="ClassesRoot"/> to delete before registering and when unregistering.</summary>
    public static IEnumerable<string> VerbKeys() =>
        ClassKeys.SelectMany(c => new[] { VerbKey(c, CompareVerb), VerbKey(c, SelectLeftVerb), VerbKey(c, LegacyCompareToLeftVerb) });

    public static string VerbKey(string classKey, string verb) => $@"{classKey}\shell\{verb}";

    /// <summary>
    /// Full HKCU subkeys that only exist in the 1.2.0 layout (static "Compare to left" verb, command-based "Select as
    /// left"). Any of them present means an old exe registered its entries again and the layout must be rewritten.
    /// </summary>
    public static IEnumerable<string> LegacyKeys() =>
        ClassKeys.SelectMany(c => new[]
        {
            $@"{ClassesRoot}\{VerbKey(c, LegacyCompareToLeftVerb)}",
            $@"{ClassesRoot}\{VerbKey(c, SelectLeftVerb)}\command",
        });

    public static string ClsidKey(string clsid) => $@"CLSID\{clsid}";

    public static string InprocServerKey(string clsid) => $@"{ClassesRoot}\{ClsidKey(clsid)}\InprocServer32";

    /// <summary>File name of the extracted DLL. The content hash keeps a DLL that Explorer still has loaded from being overwritten.</summary>
    public static string DllFileName(string hash) => $"FolderCompare.ShellExt.{hash}.dll";

    /// <summary>The values to write, as full subkeys of HKCU.</summary>
    public static IReadOnlyList<RegistryValue> Build(string exePath, string dllPath)
    {
        var icon = $"\"{exePath}\",0";
        var values = new List<RegistryValue>();
        foreach (var (clsid, name) in new[] { (CompareClsid, "FolderCompare Compare"), (SelectLeftClsid, "FolderCompare Select Left") })
        {
            values.Add(new RegistryValue($@"{ClassesRoot}\{ClsidKey(clsid)}", "", name));
            values.Add(new RegistryValue(InprocServerKey(clsid), "", dllPath));
            values.Add(new RegistryValue(InprocServerKey(clsid), "ThreadingModel", "Apartment"));
        }
        foreach (var classKey in ClassKeys)
        {
            AddVerb(values, VerbKey(classKey, CompareVerb), CompareClsid, icon);
            AddVerb(values, VerbKey(classKey, SelectLeftVerb), SelectLeftClsid, icon);
        }
        values.Add(new RegistryValue(AppKey, ExePathValue, exePath));
        return values;
    }

    private static void AddVerb(List<RegistryValue> values, string verbKey, string clsid, string icon)
    {
        var key = $@"{ClassesRoot}\{verbKey}";
        values.Add(new RegistryValue(key, "ExplorerCommandHandler", clsid));
        values.Add(new RegistryValue(key, "MultiSelectModel", "Player")); // all selected items in one call
        values.Add(new RegistryValue(key, "Icon", icon));
    }

    public static PairKind Classify(IFileSystem fs, string left, string right)
    {
        bool leftDir = fs.Directory.Exists(left), rightDir = fs.Directory.Exists(right);
        bool leftFile = !leftDir && fs.File.Exists(left), rightFile = !rightDir && fs.File.Exists(right);
        if (!(leftDir || leftFile) || !(rightDir || rightFile)) return PairKind.Missing;
        if (leftDir && rightDir) return PairKind.BothFolders;
        if (leftFile && rightFile) return PairKind.BothFiles;
        return PairKind.Mixed;
    }
}
