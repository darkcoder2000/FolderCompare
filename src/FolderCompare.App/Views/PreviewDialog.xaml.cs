using System.Windows;
using FolderCompare.App.Services;
using FolderCompare.App.ViewModels;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

public partial class PreviewDialog : Window
{
    public PreviewDialog(PreviewRequest request)
    {
        InitializeComponent();
        Title = request.Title;

        var plan = request.Plan;
        var rows = plan.Actions.Select(ToRow).ToList();
        ActionsGrid.ItemsSource = rows;

        var files = plan.Actions.Count(a => a.Kind != ActionKind.CreateDirectory);
        HeaderText.Text = request.IsDelete
            ? $"{(request.Permanent ? "Permanently delete" : "Move to the Recycle Bin")}: {plan.Actions.Count:N0} item(s), {RowViewModel.FormatSize(plan.TotalBytes)}"
            : $"{files:N0} file(s) to copy, {RowViewModel.FormatSize(plan.TotalBytes)}" +
              (plan.Actions.Count > files ? $", {plan.Actions.Count - files:N0} folder(s) to create" : "");

        int newer = plan.Actions.Count(a => a.TargetIsNewer);
        int overwrites = plan.Actions.Count(a => a.TargetExists && a.Kind == ActionKind.CopyFile);
        if (request.Permanent)
            ShowWarning("Items will be deleted permanently and cannot be restored from the Recycle Bin.");
        else if (newer > 0)
            ShowWarning($"{newer:N0} file(s) at the destination are NEWER than the source. Check the copy direction.");
        else if (overwrites > 0)
            ShowWarning($"{overwrites:N0} existing file(s) at the destination will be overwritten (subject to the overwrite policy).");

        if (plan.Notes.Count > 0)
        {
            NotesText.Text = "Not included: " + string.Join("  |  ", plan.Notes.Take(20)) + (plan.Notes.Count > 20 ? " ..." : "");
            NotesText.Visibility = Visibility.Visible;
        }

        DontAskAgain.Visibility = request.OfferDontAskAgain ? Visibility.Visible : Visibility.Collapsed;
        OkButton.Content = request.IsDelete ? "_Delete" : "_Copy";
    }

    public bool DontAskAgainChecked => DontAskAgain.IsChecked == true;

    private void ShowWarning(string text)
    {
        WarningText.Text = text;
        WarningBar.Visibility = Visibility.Visible;
    }

    private void Ok_Click(object sender, RoutedEventArgs e) => DialogResult = true;

    private static PreviewRow ToRow(PlannedAction a) => new(
        a.Kind switch
        {
            ActionKind.CopyFile => "Copy",
            ActionKind.CreateDirectory => "Create folder",
            _ => a.IsDirectory ? "Delete folder" : "Delete file",
        },
        a.SourcePath ?? "",
        a.TargetPath,
        a.Kind == ActionKind.CreateDirectory ? "" : RowViewModel.FormatSize(a.Size),
        a.Kind != ActionKind.CopyFile || !a.TargetExists ? "" : a.TargetIsNewer ? "yes - destination is newer!" : "yes",
        a.TargetIsNewer);

    public sealed record PreviewRow(string Action, string Source, string Target, string Size, string Overwrite, bool IsWarning);
}
