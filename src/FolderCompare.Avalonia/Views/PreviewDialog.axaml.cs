using Avalonia.Controls;
using Avalonia.Interactivity;
using FolderCompare.App.Services;
using FolderCompare.App.ViewModels;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

public sealed record PreviewRow(string Action, string Source, string Target, string Size, string Overwrite, bool IsWarning);

/// <summary>Lists the planned actions before a copy or delete. The dialog result is true when confirmed.</summary>
public partial class PreviewDialog : Window
{
    public PreviewDialog() => InitializeComponent();

    public PreviewDialog(PreviewRequest request, string trashName) : this()
    {
        Title = request.Title;

        var plan = request.Plan;
        ActionsGrid.ItemsSource = plan.Actions.Select(ToRow).ToList();

        var files = plan.Actions.Count(a => a.Kind != ActionKind.CreateDirectory);
        HeaderText.Text = request.IsDelete
            ? $"{(request.Permanent ? "Permanently delete" : $"Move to the {trashName}")}: {plan.Actions.Count:N0} item(s), {RowViewModel.FormatSize(plan.TotalBytes)}"
            : $"{files:N0} file(s) to copy, {RowViewModel.FormatSize(plan.TotalBytes)}" +
              (plan.Actions.Count > files ? $", {plan.Actions.Count - files:N0} folder(s) to create" : "");

        int newer = plan.Actions.Count(a => a.TargetIsNewer);
        int unknown = plan.Actions.Count(a => a.DirectionUnknown);
        int overwrites = plan.Actions.Count(a => a.TargetExists && a.Kind == ActionKind.CopyFile);
        if (request.Permanent)
            ShowWarning($"Items will be deleted permanently and cannot be restored from the {trashName}.");
        else if (newer > 0)
            ShowWarning($"{newer:N0} file(s) at the destination are NEWER than the source. Check the copy direction.");
        else if (unknown > 0)
            ShowWarning($"{unknown:N0} file(s) have the same timestamp but different content. You will be asked before each is overwritten.");
        else if (overwrites > 0)
            ShowWarning($"{overwrites:N0} existing file(s) at the destination will be overwritten (subject to the overwrite policy).");

        if (plan.Notes.Count > 0)
        {
            NotesText.Text = "Not included: " + string.Join("  |  ", plan.Notes.Take(20)) + (plan.Notes.Count > 20 ? " ..." : "");
            NotesText.IsVisible = true;
        }

        DontAskAgain.Content = $"_Don't ask again for {trashName} deletes";
        DontAskAgain.IsVisible = request.OfferDontAskAgain;
        OkButton.Content = request.IsDelete ? "_Delete" : "_Copy";
    }

    public bool DontAskAgainChecked => DontAskAgain.IsChecked == true;

    private void ShowWarning(string text)
    {
        WarningText.Text = text;
        WarningBar.IsVisible = true;
    }

    private void Ok_Click(object? sender, RoutedEventArgs e) => Close(true);
    private void Cancel_Click(object? sender, RoutedEventArgs e) => Close(false);

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
        a.Kind != ActionKind.CopyFile || !a.TargetExists ? ""
            : a.TargetIsNewer ? "yes - destination is newer!"
            : a.DirectionUnknown ? "ask - same time, different content!"
            : "yes",
        a.TargetIsNewer || a.DirectionUnknown);
}
