using FolderCompare.Core.Comparison;

namespace FolderCompare.Core.Models;

public enum SymlinkMode
{
    Skip,
    Follow,
}

public enum ContentCompareMode
{
    /// <summary>Timestamp + size only (default, fast).</summary>
    Off,
    /// <summary>Verify the content of files that look identical by timestamp and size.</summary>
    WhenTimestampsEqual,
    /// <summary>Also verify files whose timestamp differs but whose size matches.</summary>
    WhenSizesEqual,
}

public sealed record CompareOptions
{
    public bool Recursive { get; init; } = true;
    public TimeSpan Tolerance { get; init; } = TimeSpan.FromSeconds(2);
    /// <summary>When set, a difference of exactly one hour (± tolerance) counts as identical.</summary>
    public bool IgnoreDstOffset { get; init; }
    public SymlinkMode SymlinkMode { get; init; } = SymlinkMode.Skip;
    /// <summary>Semicolon-separated glob patterns; when non-empty only matching files are compared.</summary>
    public string IncludePatterns { get; init; } = "";
    /// <summary>Semicolon-separated glob patterns for files and folders to skip.</summary>
    public string ExcludePatterns { get; init; } = "";
    public bool IgnoreHiddenAndSystem { get; init; }
    public ContentCompareMode ContentCompare { get; init; } = ContentCompareMode.Off;
    /// <summary>
    /// How names are matched. Null = decided per compare from the OS and the roots (see
    /// <see cref="Comparison.PathComparison.ForRoots"/>); the result's options always carry the resolved value.
    /// </summary>
    public PathComparison? PathComparison { get; init; }

    /// <summary>The effective name comparison.</summary>
    public PathComparison Paths => PathComparison ?? Comparison.PathComparison.ForCurrentOS();
}
