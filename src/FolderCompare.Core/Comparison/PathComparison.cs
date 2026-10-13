using System.IO.Abstractions;

namespace FolderCompare.Core.Comparison;

/// <summary>
/// How file and folder names are matched between the two trees: case-insensitive on Windows and macOS (default file
/// systems), case-sensitive on Linux. On Linux <c>a.txt</c> and <c>A.txt</c> are two different files.
/// </summary>
public sealed class PathComparison
{
    public static readonly PathComparison IgnoreCase = new(true);
    public static readonly PathComparison CaseSensitive = new(false);

    private PathComparison(bool ignoreCase)
    {
        IsIgnoreCase = ignoreCase;
        Comparer = ignoreCase ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal;
        Comparison = ignoreCase ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
    }

    public bool IsIgnoreCase { get; }
    public StringComparer Comparer { get; }
    public StringComparison Comparison { get; }

    /// <summary>The usual rule of the current operating system.</summary>
    public static PathComparison ForCurrentOS() => OperatingSystem.IsLinux() ? CaseSensitive : IgnoreCase;

    /// <summary>
    /// The OS default, made case-sensitive when one of the roots turns out to be on a case-sensitive file system
    /// (e.g. a case-sensitive APFS volume or a case-sensitive folder on Windows). Case-sensitive is the stricter rule:
    /// it never merges two different files into one row.
    /// </summary>
    public static PathComparison ForRoots(IFileSystem fs, params string[] roots)
    {
        var result = ForCurrentOS();
        if (!result.IsIgnoreCase) return result;
        return roots.Any(r => IsCaseSensitive(fs, r)) ? CaseSensitive : result;
    }

    /// <summary>
    /// Read-only probe: flips the case of the last path segment and checks whether it still resolves. If it does not,
    /// the file system is case-sensitive. Roots without letters can't be probed and keep the OS default.
    /// </summary>
    internal static bool IsCaseSensitive(IFileSystem fs, string root)
    {
        try
        {
            var full = fs.Path.TrimEndingDirectorySeparator(fs.Path.GetFullPath(root));
            var name = fs.Path.GetFileName(full);
            if (string.IsNullOrEmpty(name)) return false;
            var flipped = new string(name.Select(c => char.IsUpper(c) ? char.ToLowerInvariant(c) : char.ToUpperInvariant(c)).ToArray());
            if (flipped == name) return false;
            var probe = fs.Path.Join(fs.Path.GetDirectoryName(full), flipped);
            return fs.Directory.Exists(full) && !fs.Directory.Exists(probe);
        }
        catch (Exception ex) when (DirectoryScanner.IsIoError(ex))
        {
            return false;
        }
    }

    public override string ToString() => IsIgnoreCase ? "IgnoreCase" : "CaseSensitive";
}
