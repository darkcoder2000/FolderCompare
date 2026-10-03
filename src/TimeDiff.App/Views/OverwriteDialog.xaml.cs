using System.Globalization;
using System.Windows;
using TimeDiff.App.ViewModels;
using TimeDiff.Core.Operations;

namespace TimeDiff.App.Views;

public partial class OverwriteDialog : Window
{
    public OverwriteDialog(OverwriteConflict conflict)
    {
        InitializeComponent();
        SourcePath.Text = conflict.SourcePath;
        TargetPath.Text = conflict.TargetPath;
        SourceInfo.Text = Describe(conflict.SourceSize, conflict.SourceTimeUtc);
        TargetInfo.Text = Describe(conflict.TargetSize, conflict.TargetTimeUtc);
        NewerWarning.Visibility = conflict.TargetIsNewer ? Visibility.Visible : Visibility.Collapsed;
        // A newer destination defaults to the safe choice.
        if (conflict.TargetIsNewer) SkipButton.IsDefault = true;
        else OverwriteButton.IsDefault = true;
    }

    public OverwriteChoice Choice { get; private set; } = OverwriteChoice.Cancel;
    public bool ApplyToAllChecked => ApplyToAll.IsChecked == true;

    private static string Describe(long size, DateTime utc) =>
        $"{RowViewModel.FormatSize(size)}, modified {utc.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture)}";

    private void Finish(OverwriteChoice choice)
    {
        Choice = choice;
        DialogResult = true;
    }

    private void Overwrite_Click(object sender, RoutedEventArgs e) => Finish(OverwriteChoice.Overwrite);
    private void OverwriteIfNewer_Click(object sender, RoutedEventArgs e) => Finish(OverwriteChoice.OverwriteIfNewer);
    private void KeepBoth_Click(object sender, RoutedEventArgs e) => Finish(OverwriteChoice.KeepBoth);
    private void Skip_Click(object sender, RoutedEventArgs e) => Finish(OverwriteChoice.Skip);
}
