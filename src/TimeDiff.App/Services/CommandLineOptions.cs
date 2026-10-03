namespace TimeDiff.App.Services;

/// <summary><c>TimeDiff.exe "C:\A" "D:\B" [--compare] [--no-recursive]</c></summary>
public sealed record CommandLineOptions(string? Left, string? Right, bool Compare, bool NoRecursive)
{
    public static CommandLineOptions Parse(IReadOnlyList<string> args)
    {
        var positional = new List<string>();
        bool compare = false, noRecursive = false;
        foreach (var arg in args)
        {
            if (string.Equals(arg, "--compare", StringComparison.OrdinalIgnoreCase)) compare = true;
            else if (string.Equals(arg, "--no-recursive", StringComparison.OrdinalIgnoreCase)) noRecursive = true;
            else if (!arg.StartsWith("--", StringComparison.Ordinal)) positional.Add(arg);
        }
        return new CommandLineOptions(positional.ElementAtOrDefault(0), positional.ElementAtOrDefault(1), compare, noRecursive);
    }
}
