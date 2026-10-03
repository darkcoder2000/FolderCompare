namespace TimeDiff.Core.Models;

public enum SymlinkMode
{
    Skip,
    Follow,
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
}
