using Avalonia.Controls;
using Avalonia.Interactivity;
using FolderCompare.App.Services;
using FolderCompare.Core.Models;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

/// <summary>Edits a copy of the settings. The dialog result is true when OK was pressed.</summary>
public partial class SettingsDialog : Window
{
    private static readonly (ContentCompareMode Mode, string Text)[] ContentModes =
    [
        (ContentCompareMode.Off, "Off (fast)"),
        (ContentCompareMode.WhenTimestampsEqual, "Verify files that look identical"),
        (ContentCompareMode.WhenSizesEqual, "Verify all same-size files"),
    ];

    private readonly AppSettings? _settings;

    public SettingsDialog() => InitializeComponent();

    public SettingsDialog(AppSettings editable, string trashName) : this()
    {
        _settings = editable;
        DataContext = editable;
        ContentCombo.ItemsSource = ContentModes.Select(m => m.Text).ToList();
        ContentCombo.SelectedIndex = Math.Max(0, Array.FindIndex(ContentModes, m => m.Mode == editable.ContentCompare));
        SymlinkCombo.ItemsSource = Enum.GetValues<SymlinkMode>();
        SymlinkCombo.SelectedItem = editable.SymlinkMode;
        OverwriteCombo.ItemsSource = Enum.GetValues<OverwritePolicy>();
        OverwriteCombo.SelectedItem = editable.OverwritePolicy;
        RecycleCheck.Content = $"Delete to the _{trashName} (unchecked = delete permanently)";
        ConfirmRecycleCheck.Content = $"_Confirm {trashName} deletes";
        if (!OperatingSystem.IsWindows()) HiddenCheck.Content = "Ignore _hidden files (names starting with a dot)";
    }

    private void Ok_Click(object? sender, RoutedEventArgs e)
    {
        if (_settings is null) return;
        _settings.ContentCompare = ContentModes[Math.Max(0, ContentCombo.SelectedIndex)].Mode;
        if (SymlinkCombo.SelectedItem is SymlinkMode symlink) _settings.SymlinkMode = symlink;
        if (OverwriteCombo.SelectedItem is OverwritePolicy policy) _settings.OverwritePolicy = policy;
        _settings.ToleranceSeconds = Math.Max(0, _settings.ToleranceSeconds);
        _settings.PreviewThreshold = Math.Max(0, _settings.PreviewThreshold);
        Close(true);
    }

    private void Cancel_Click(object? sender, RoutedEventArgs e) => Close(false);
}
