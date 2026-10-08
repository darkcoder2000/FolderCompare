using System.IO.Abstractions;

namespace FolderCompare.Core.Shell;

/// <summary>One registry value below HKCU\Software\Classes. An empty <see cref="Name"/> is the key's default value.</summary>
public sealed record RegistryValue(string SubKey, string Name, string Value);

public enum PairKind
{
    BothFolders,
    BothFiles,
    Mixed,
    Missing,
}

/// <summary>The Explorer context menu entries ("Select as left" / "Compare to left") as registry values.</summary>
public static class ExplorerMenuLayout
{
    public const string SelectLeftSwitch = "--select-left";
    public const string CompareToLeftSwitch = "--compare-to-left";

    public const string SelectLeftVerb = "FolderCompare.SelectLeft";
    public const string CompareToLeftVerb = "FolderCompare.CompareToLeft";

    public const string SelectLeftLabel = "Select as left for FolderCompare";
    public const string CompareToLeftLabel = "Compare to left with FolderCompare";

    /// <summary>Files and folders.</summary>
    public static readonly IReadOnlyList<string> ClassKeys = new[] { "*", "Directory" };

    /// <summary>The verb keys to delete when unregistering.</summary>
    public static IEnumerable<string> VerbKeys() =>
        ClassKeys.SelectMany(c => new[] { VerbKey(c, SelectLeftVerb), VerbKey(c, CompareToLeftVerb) });

    public static string VerbKey(string classKey, string verb) => $@"{classKey}\shell\{verb}";

    public static string Command(string exePath, string @switch) => $"\"{exePath}\" {@switch} \"%1\"";

    /// <summary>Label of the second entry. Shows the name of the pending left item when there is one.</summary>
    public static string CompareLabel(string? pendingLeft)
    {
        if (string.IsNullOrWhiteSpace(pendingLeft)) return CompareToLeftLabel;
        var name = Path.GetFileName(pendingLeft.TrimEnd('\\', '/'));
        if (string.IsNullOrEmpty(name)) name = pendingLeft; // drive root like C:\
        return $"Compare to \"{name}\" with FolderCompare";
    }

    public static IReadOnlyList<RegistryValue> Build(string exePath, string? pendingLeft)
    {
        var icon = $"\"{exePath}\",0";
        var values = new List<RegistryValue>();
        foreach (var classKey in ClassKeys)
        {
            Add(values, VerbKey(classKey, SelectLeftVerb), SelectLeftLabel, icon, Command(exePath, SelectLeftSwitch));
            Add(values, VerbKey(classKey, CompareToLeftVerb), CompareLabel(pendingLeft), icon, Command(exePath, CompareToLeftSwitch));
        }
        return values;
    }

    private static void Add(List<RegistryValue> values, string verbKey, string label, string icon, string command)
    {
        values.Add(new RegistryValue(verbKey, "MUIVerb", label));
        values.Add(new RegistryValue(verbKey, "Icon", icon));
        values.Add(new RegistryValue(verbKey + @"\command", "", command));
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
