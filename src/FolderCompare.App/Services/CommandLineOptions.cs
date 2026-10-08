using FolderCompare.Core.Shell;

namespace FolderCompare.App.Services;

/// <summary>
/// <c>FolderCompare.exe "C:\A" "D:\B" [--compare] [--no-recursive]</c>, or from the Explorer context menu
/// <c>--select-left "path"</c> / <c>--compare-to-left "path"</c>.
/// </summary>
public sealed record CommandLineOptions(string? Left, string? Right, bool Compare, bool NoRecursive)
{
    public string? SelectLeft { get; init; }
    public string? CompareToLeft { get; init; }

    public static CommandLineOptions Parse(IReadOnlyList<string> args)
    {
        var positional = new List<string>();
        bool compare = false, noRecursive = false;
        string? selectLeft = null, compareToLeft = null;
        for (int i = 0; i < args.Count; i++)
        {
            var arg = args[i];
            if (string.Equals(arg, "--compare", StringComparison.OrdinalIgnoreCase)) compare = true;
            else if (string.Equals(arg, "--no-recursive", StringComparison.OrdinalIgnoreCase)) noRecursive = true;
            else if (string.Equals(arg, ExplorerMenuLayout.SelectLeftSwitch, StringComparison.OrdinalIgnoreCase))
                selectLeft = i + 1 < args.Count ? args[++i] : null;
            else if (string.Equals(arg, ExplorerMenuLayout.CompareToLeftSwitch, StringComparison.OrdinalIgnoreCase))
                compareToLeft = i + 1 < args.Count ? args[++i] : null;
            else if (!arg.StartsWith("--", StringComparison.Ordinal)) positional.Add(arg);
        }
        return new CommandLineOptions(positional.ElementAtOrDefault(0), positional.ElementAtOrDefault(1), compare, noRecursive)
        {
            SelectLeft = selectLeft,
            CompareToLeft = compareToLeft,
        };
    }
}
