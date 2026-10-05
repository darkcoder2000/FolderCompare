using System.IO.Abstractions;

namespace FolderCompare.Core.Operations;

public sealed class UnsafePathException : Exception
{
    public UnsafePathException(string message) : base(message) { }
}

/// <summary>Guards against operating outside the chosen roots and against invalid root combinations.</summary>
public static class SafetyValidator
{
    public static string Normalize(IFileSystem fs, string path) =>
        fs.Path.TrimEndingDirectorySeparator(fs.Path.GetFullPath(path.Trim()));

    /// <summary>Returns an error message, or null if the pair of roots may be compared.</summary>
    public static string? ValidateRoots(IFileSystem fs, string? left, string? right)
    {
        if (string.IsNullOrWhiteSpace(left) || string.IsNullOrWhiteSpace(right)) return "Please choose both folders.";

        string l, r;
        try
        {
            l = Normalize(fs, left);
            r = Normalize(fs, right);
        }
        catch (Exception ex) when (ex is ArgumentException or NotSupportedException or PathTooLongException)
        {
            return "Invalid path: " + ex.Message;
        }

        if (!fs.Directory.Exists(l)) return $"Left folder does not exist or is not accessible: {l}";
        if (!fs.Directory.Exists(r)) return $"Right folder does not exist or is not accessible: {r}";
        if (string.Equals(l, r, StringComparison.OrdinalIgnoreCase)) return "Left and right are the same folder.";
        if (IsInside(l, r) || IsInside(r, l)) return "One folder is inside the other. Choose two independent folders.";
        return null;
    }

    /// <summary>True if <paramref name="fullPath"/> is strictly below <paramref name="root"/> (both already normalized).</summary>
    public static bool IsInside(string root, string fullPath)
    {
        var prefix = root.EndsWith('\\') ? root : root + "\\";
        return fullPath.Length > prefix.Length && fullPath.StartsWith(prefix, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>Resolves <paramref name="path"/> and throws unless it lies strictly inside <paramref name="root"/>.</summary>
    public static string EnsureInsideRoot(IFileSystem fs, string root, string path)
    {
        string full, normalizedRoot;
        try
        {
            full = fs.Path.GetFullPath(path);
            normalizedRoot = Normalize(fs, root);
        }
        catch (Exception ex) when (ex is ArgumentException or NotSupportedException or PathTooLongException)
        {
            throw new UnsafePathException($"Invalid path '{path}': {ex.Message}");
        }

        if (!IsInside(normalizedRoot, full))
            throw new UnsafePathException($"Refusing to operate on '{full}' because it is outside the root '{normalizedRoot}'.");
        return full;
    }
}
