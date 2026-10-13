using System.Globalization;
using Avalonia.Controls;
using Avalonia.Interactivity;
using FolderCompare.App.ViewModels;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

/// <summary>Asks what to do with an existing destination file. Closing the window means "Cancel all".</summary>
public partial class OverwriteDialog : Window
{
    public OverwriteDialog() => InitializeComponent();

    public OverwriteDialog(OverwriteConflict conflict) : this()
    {
        SourcePath.Text = conflict.SourcePath;
        TargetPath.Text = conflict.TargetPath;
        SourceInfo.Text = Describe(conflict.SourceSize, conflict.SourceTimeUtc);
        TargetInfo.Text = Describe(conflict.TargetSize, conflict.TargetTimeUtc);
        NewerWarning.IsVisible = conflict.TargetIsNewer;
        UnknownWarning.IsVisible = conflict.DirectionUnknown;
        // A newer destination or an unknown direction defaults to the safe choice.
        var defaultButton = conflict.TargetIsNewer || conflict.DirectionUnknown ? SkipButton : OverwriteButton;
        defaultButton.IsDefault = true;
        Opened += (_, _) => defaultButton.Focus();
    }

    private static string Describe(long size, DateTime utc) =>
        $"{RowViewModel.FormatSize(size)}, modified {utc.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture)}";

    private void Finish(OverwriteChoice choice) => Close(new OverwriteDecision(choice, ApplyToAll.IsChecked == true));

    private void Overwrite_Click(object? sender, RoutedEventArgs e) => Finish(OverwriteChoice.Overwrite);
    private void OverwriteIfNewer_Click(object? sender, RoutedEventArgs e) => Finish(OverwriteChoice.OverwriteIfNewer);
    private void KeepBoth_Click(object? sender, RoutedEventArgs e) => Finish(OverwriteChoice.KeepBoth);
    private void Skip_Click(object? sender, RoutedEventArgs e) => Finish(OverwriteChoice.Skip);
    private void Cancel_Click(object? sender, RoutedEventArgs e) => Close(new OverwriteDecision(OverwriteChoice.Cancel, false));
}
