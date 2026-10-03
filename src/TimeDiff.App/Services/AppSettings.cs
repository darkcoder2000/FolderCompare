using TimeDiff.Core.Models;
using TimeDiff.Core.Operations;

namespace TimeDiff.App.Services;

public sealed class AppSettings
{
    // Comparison
    public double ToleranceSeconds { get; set; } = 2;
    public bool IgnoreDstOffset { get; set; }
    public SymlinkMode SymlinkMode { get; set; } = SymlinkMode.Skip;
    public bool IgnoreHiddenAndSystem { get; set; }
    public bool Recursive { get; set; } = true;
    public string ExcludePatterns { get; set; } = "";
    public string IncludePatterns { get; set; } = "";

    // Operations
    public OverwritePolicy OverwritePolicy { get; set; } = OverwritePolicy.Ask;
    public bool DeleteToRecycleBin { get; set; } = true;
    public bool ConfirmRecycleDelete { get; set; } = true;
    /// <summary>The copy preview is skipped when the batch has fewer than this many actions (0 = always preview).</summary>
    public int PreviewThreshold { get; set; }

    // View
    public bool FoldersFirst { get; set; } = true;
    public FilterSettings Filters { get; set; } = new();

    // Session
    public string LastLeft { get; set; } = "";
    public string LastRight { get; set; } = "";
    public List<string> RecentPaths { get; set; } = new();
    public WindowSettings? Window { get; set; }
    public List<double>? ColumnWidths { get; set; }
    public List<Profile> Profiles { get; set; } = new();

    public CompareOptions ToCompareOptions() => new()
    {
        Recursive = Recursive,
        Tolerance = TimeSpan.FromSeconds(Math.Max(0, ToleranceSeconds)),
        IgnoreDstOffset = IgnoreDstOffset,
        SymlinkMode = SymlinkMode,
        IncludePatterns = IncludePatterns,
        ExcludePatterns = ExcludePatterns,
        IgnoreHiddenAndSystem = IgnoreHiddenAndSystem,
    };
}

public sealed class FilterSettings
{
    public bool ShowOnlyLeft { get; set; } = true;
    public bool ShowOnlyRight { get; set; } = true;
    public bool ShowNewerLeft { get; set; } = true;
    public bool ShowNewerRight { get; set; } = true;
    public bool ShowIdentical { get; set; } = true;
    public bool ShowErrors { get; set; } = true;
    public bool DifferencesOnly { get; set; }

    public FilterSettings Clone() => (FilterSettings)MemberwiseClone();
}

public sealed class WindowSettings
{
    public double Left { get; set; }
    public double Top { get; set; }
    public double Width { get; set; }
    public double Height { get; set; }
    public bool Maximized { get; set; }
}

/// <summary>A named folder pair plus the comparison options and filters.</summary>
public sealed class Profile
{
    public string Name { get; set; } = "";
    public string LeftPath { get; set; } = "";
    public string RightPath { get; set; } = "";
    public double ToleranceSeconds { get; set; } = 2;
    public bool IgnoreDstOffset { get; set; }
    public bool IgnoreHiddenAndSystem { get; set; }
    public bool Recursive { get; set; } = true;
    public string ExcludePatterns { get; set; } = "";
    public string IncludePatterns { get; set; } = "";
    public FilterSettings Filters { get; set; } = new();

    public override string ToString() => Name;
}
