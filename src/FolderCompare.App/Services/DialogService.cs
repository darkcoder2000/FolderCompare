using System.IO;
using System.Windows;
using Microsoft.Win32;
using FolderCompare.App.Views;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Services;

public sealed class DialogService : IDialogService
{
    private static Window? Owner => Application.Current.MainWindow is { IsLoaded: true } w ? w : null;

    private static bool? Show(Window dialog)
    {
        dialog.Owner = Owner;
        dialog.Icon ??= dialog.Owner?.Icon;
        if (dialog.Owner is null) dialog.WindowStartupLocation = WindowStartupLocation.CenterScreen;
        return dialog.ShowDialog();
    }

    public Task<string?> PickFolderAsync(string? initialPath) => Task.FromResult(PickFolder(initialPath));
    public Task<string?> PickSaveFileAsync(string defaultName, IReadOnlyList<FileType> types) =>
        Task.FromResult(PickSaveFile(defaultName, string.Join("|", types.Select(t => $"{t.Name} (*.{t.Extension})|*.{t.Extension}"))));
    public Task<PreviewResult> ShowPreviewAsync(PreviewRequest request) => Task.FromResult(ShowPreview(request));
    public Task<OverwriteDecision> AskOverwriteAsync(OverwriteConflict conflict) => Task.FromResult(AskOverwrite(conflict));
    public Task ShowSummaryAsync(string title, OperationSummary summary, IReadOnlyList<string> notes)
    {
        ShowSummary(title, summary, notes);
        return Task.CompletedTask;
    }
    public Task<Sides> AskDeleteSideAsync() => Task.FromResult(AskDeleteSide());
    public Task<RenameResult?> AskRenameAsync(RenameRequest request) => Task.FromResult(AskRename(request));
    public Task<string?> AskTextAsync(string title, string prompt, string initial) => Task.FromResult(AskText(title, prompt, initial));
    public Task<AppSettings?> EditSettingsAsync(AppSettings current) => Task.FromResult(EditSettings(current));
    public Task<bool> ConfirmAsync(string title, string message, bool warning = false) => Task.FromResult(Confirm(title, message, warning));
    public Task ShowInfoAsync(string title, string message)
    {
        ShowInfo(title, message);
        return Task.CompletedTask;
    }
    public Task ShowErrorAsync(string title, string message)
    {
        ShowError(title, message);
        return Task.CompletedTask;
    }

    public string? PickFolder(string? initialPath)
    {
        var dialog = new OpenFolderDialog { Title = "Choose a folder" };
        if (!string.IsNullOrWhiteSpace(initialPath) && Directory.Exists(initialPath)) dialog.InitialDirectory = initialPath;
        return dialog.ShowDialog(Owner) == true ? dialog.FolderName : null;
    }

    public string? PickSaveFile(string defaultName, string filter)
    {
        var dialog = new SaveFileDialog { FileName = defaultName, Filter = filter, AddExtension = true };
        return dialog.ShowDialog(Owner) == true ? dialog.FileName : null;
    }

    public PreviewResult ShowPreview(PreviewRequest request)
    {
        var dialog = new PreviewDialog(request);
        return Show(dialog) == true ? new PreviewResult(true, dialog.DontAskAgainChecked) : new PreviewResult(false, false);
    }

    public OverwriteDecision AskOverwrite(OverwriteConflict conflict)
    {
        var dialog = new OverwriteDialog(conflict);
        return Show(dialog) == true
            ? new OverwriteDecision(dialog.Choice, dialog.ApplyToAllChecked)
            : new OverwriteDecision(OverwriteChoice.Cancel, false);
    }

    public void ShowSummary(string title, OperationSummary summary, IReadOnlyList<string> notes) =>
        Show(new SummaryDialog(title, summary, notes));

    public Sides AskDeleteSide()
    {
        var dialog = new DeleteSideDialog();
        return Show(dialog) == true ? dialog.Sides : Sides.None;
    }

    public RenameResult? AskRename(RenameRequest request)
    {
        var dialog = new InputDialog(request);
        return Show(dialog) == true ? new RenameResult(dialog.Value, dialog.LeftChecked, dialog.RightChecked) : null;
    }

    public string? AskText(string title, string prompt, string initial)
    {
        var dialog = new InputDialog(title, prompt, initial);
        return Show(dialog) == true ? dialog.Value : null;
    }

    public AppSettings? EditSettings(AppSettings current)
    {
        var copy = SettingsService.Clone(current);
        return Show(new SettingsDialog(copy)) == true ? copy : null;
    }

    public bool Confirm(string title, string message, bool warning = false) =>
        Message(message, title, MessageBoxButton.YesNo, warning ? MessageBoxImage.Warning : MessageBoxImage.Question,
                MessageBoxResult.No) == MessageBoxResult.Yes;

    public void ShowInfo(string title, string message) =>
        Message(message, title, MessageBoxButton.OK, MessageBoxImage.Information);

    public void ShowError(string title, string message) =>
        Message(message, title, MessageBoxButton.OK, MessageBoxImage.Error);

    private static MessageBoxResult Message(string text, string caption, MessageBoxButton buttons, MessageBoxImage image,
                                            MessageBoxResult defaultResult = MessageBoxResult.OK) =>
        Owner is { } owner
            ? MessageBox.Show(owner, text, caption, buttons, image, defaultResult)
            : MessageBox.Show(text, caption, buttons, image, defaultResult);
}
