using FolderCompare.Core.Models;

namespace FolderCompare.App.Services;

/// <summary>
/// <c>FolderCompare.exe "C:\A" "D:\B" [--compare] [--no-recursive] [--content off|equal-time|equal-size]</c>.
/// Two files open the text compare window (used by the Explorer "Compare Files" entry).
/// </summary>
public sealed record CommandLineOptions(string? Left, string? Right, bool Compare, bool NoRecursive,
                                        ContentCompareMode? ContentCompare = null)
{
    public static CommandLineOptions Parse(IReadOnlyList<string> args)
    {
        var positional = new List<string>();
        bool compare = false, noRecursive = false;
        ContentCompareMode? content = null;
        for (int i = 0; i < args.Count; i++)
        {
            var arg = args[i];
            if (string.Equals(arg, "--compare", StringComparison.OrdinalIgnoreCase)) compare = true;
            else if (string.Equals(arg, "--no-recursive", StringComparison.OrdinalIgnoreCase)) noRecursive = true;
            else if (string.Equals(arg, "--content", StringComparison.OrdinalIgnoreCase))
            {
                if (i + 1 < args.Count) content = ParseContentMode(args[++i]) ?? content;
            }
            else if (arg.StartsWith("--content=", StringComparison.OrdinalIgnoreCase))
            {
                content = ParseContentMode(arg["--content=".Length..]) ?? content;
            }
            else if (!arg.StartsWith("--", StringComparison.Ordinal)) positional.Add(arg);
        }
        return new CommandLineOptions(positional.ElementAtOrDefault(0), positional.ElementAtOrDefault(1), compare, noRecursive, content);
    }

    private static ContentCompareMode? ParseContentMode(string value) => value.ToLowerInvariant() switch
    {
        "off" => ContentCompareMode.Off,
        "equal-time" => ContentCompareMode.WhenTimestampsEqual,
        "equal-size" => ContentCompareMode.WhenSizesEqual,
        _ => null,
    };
}
