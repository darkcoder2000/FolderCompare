using System.Globalization;
using System.Windows;
using CommunityToolkit.Mvvm.ComponentModel;
using TimeDiff.Core.Models;

namespace TimeDiff.App.ViewModels;

public enum SideState
{
    Missing,
    Normal,
    Newer,
    Older,
    Only,
    Error,
}

/// <summary>One visible row of the side-by-side tree grid; wraps a <see cref="DiffNode"/>.</summary>
public sealed class RowViewModel : ObservableObject
{
    private const string FolderGlyph = "";
    private const string FileGlyph = "";
    private const string WarningGlyph = "";
    private const double IndentWidth = 16;

    private readonly MainViewModel _owner;
    private int _level;

    public RowViewModel(DiffNode node, int level, MainViewModel owner)
    {
        Node = node;
        _level = level;
        _owner = owner;
    }

    public DiffNode Node { get; }

    public int Level
    {
        get => _level;
        set
        {
            if (SetProperty(ref _level, value)) OnPropertyChanged(nameof(Indent));
        }
    }

    public Thickness Indent => new(_level * IndentWidth, 0, 0, 0);
    public bool IsExpandable => Node.IsDirectory && Node.Children.Count > 0;

    public bool IsExpanded
    {
        get => _owner.IsExpanded(Node);
        set
        {
            if (value != IsExpanded) _owner.SetExpanded(this, value);
        }
    }

    public string RelativePath => Node.RelativePath;
    public string LeftName => Node.Left is null ? "" : Node.Name;
    public string RightName => Node.Right is null ? "" : Node.Name;
    public string LeftSize => Node.Left is null ? "" : FormatSize(Node.LeftTotalSize);
    public string RightSize => Node.Right is null ? "" : FormatSize(Node.RightTotalSize);
    public string LeftDate => Node.Left is null ? "" : FormatDate(Node.Left.LastWriteUtc);
    public string RightDate => Node.Right is null ? "" : FormatDate(Node.Right.LastWriteUtc);
    public string LeftIcon => Icon(Node.Left);
    public string RightIcon => Icon(Node.Right);
    public SideState LeftState => State(Side.Left);
    public SideState RightState => State(Side.Right);
    public bool ShowDiffMarker => Node.IsDirectory && Node.Status == DiffStatus.Differs;
    public string? ToolTip => Node.ErrorMessage;

    public string Glyph => Node.Status switch
    {
        DiffStatus.NewerLeft or DiffStatus.OnlyLeft => "→",
        DiffStatus.NewerRight or DiffStatus.OnlyRight => "←",
        DiffStatus.Identical => "=",
        DiffStatus.Differs => "≠",
        _ => "⚠",
    };

    public void Refresh() => OnPropertyChanged(string.Empty);

    internal void NotifyExpanded() => OnPropertyChanged(nameof(IsExpanded));

    private string Icon(FileInfoSnapshot? snap)
    {
        if (snap is null) return "";
        if (Node.Status is DiffStatus.Error or DiffStatus.TypeConflict) return WarningGlyph;
        return snap.IsDirectory ? FolderGlyph : FileGlyph;
    }

    private SideState State(Side side)
    {
        if (Node.Get(side) is null) return SideState.Missing;
        return Node.Status switch
        {
            DiffStatus.Error or DiffStatus.TypeConflict => SideState.Error,
            DiffStatus.OnlyLeft or DiffStatus.OnlyRight => SideState.Only,
            DiffStatus.NewerLeft => side == Side.Left ? SideState.Newer : SideState.Older,
            DiffStatus.NewerRight => side == Side.Right ? SideState.Newer : SideState.Older,
            _ => SideState.Normal,
        };
    }

    public static string FormatSize(long bytes)
    {
        string[] units = ["B", "KB", "MB", "GB", "TB"];
        double value = bytes;
        int unit = 0;
        while (value >= 1024 && unit < units.Length - 1)
        {
            value /= 1024;
            unit++;
        }
        return unit == 0 ? $"{bytes:N0} B" : $"{value.ToString("0.0", CultureInfo.CurrentCulture)} {units[unit]}";
    }

    private static string FormatDate(DateTime utc) =>
        utc.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture);
}
