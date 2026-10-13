using Avalonia.Controls;
using Avalonia.Platform.Storage;
using FolderCompare.App.Views;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Services;

public sealed class DialogService : IDialogService
{
    private readonly IShellService _shell;

    public DialogService(IShellService shell) => _shell = shell;

    /// <summary>The window dialogs belong to: the active window, else the main window.</summary>
    private static Window? Owner => ShellService.ActiveWindow() is { IsVisible: true } w ? w : null;

    private static async Task<T?> ShowAsync<T>(Window dialog)
    {
        if (Owner is { } owner) return await dialog.ShowDialog<T?>(owner);

        // No window yet (e.g. a message before the main window opens): show it on its own and wait for it.
        dialog.WindowStartupLocation = WindowStartupLocation.CenterScreen;
        dialog.ShowInTaskbar = true;
        var closed = new TaskCompletionSource<T?>();
        dialog.Closed += (_, _) => closed.TrySetResult(default);
        dialog.Show();
        return await closed.Task;
    }

    public async Task<string?> PickFolderAsync(string? initialPath)
    {
        if (Owner is not { } owner) return null;
        var options = new FolderPickerOpenOptions { Title = "Choose a folder", AllowMultiple = false };
        if (!string.IsNullOrWhiteSpace(initialPath) && Directory.Exists(initialPath))
            options.SuggestedStartLocation = await owner.StorageProvider.TryGetFolderFromPathAsync(initialPath);
        var folders = await owner.StorageProvider.OpenFolderPickerAsync(options);
        return folders.Count > 0 ? folders[0].TryGetLocalPath() : null;
    }

    public async Task<string?> PickSaveFileAsync(string defaultName, IReadOnlyList<FileType> types)
    {
        if (Owner is not { } owner) return null;
        var file = await owner.StorageProvider.SaveFilePickerAsync(new FilePickerSaveOptions
        {
            SuggestedFileName = defaultName,
            DefaultExtension = types.FirstOrDefault()?.Extension,
            FileTypeChoices = types.Select(t => new FilePickerFileType(t.Name) { Patterns = [$"*.{t.Extension}"] }).ToList(),
            ShowOverwritePrompt = true,
        });
        return file?.TryGetLocalPath();
    }

    public async Task<PreviewResult> ShowPreviewAsync(PreviewRequest request)
    {
        var dialog = new PreviewDialog(request, _shell.TrashName);
        return await ShowAsync<bool>(dialog) ? new PreviewResult(true, dialog.DontAskAgainChecked) : new PreviewResult(false, false);
    }

    public async Task<OverwriteDecision> AskOverwriteAsync(OverwriteConflict conflict) =>
        // Closing the window without a choice cancels the whole operation.
        await ShowAsync<OverwriteDecision>(new OverwriteDialog(conflict)) ?? new OverwriteDecision(OverwriteChoice.Cancel, false);

    public Task ShowSummaryAsync(string title, OperationSummary summary, IReadOnlyList<string> notes) =>
        ShowAsync<object>(new SummaryDialog(title, summary, notes));

    public async Task<Sides> AskDeleteSideAsync() => await ShowAsync<Sides>(new DeleteSideDialog());

    public async Task<RenameResult?> AskRenameAsync(RenameRequest request)
    {
        var dialog = new InputDialog(request);
        return await ShowAsync<string>(dialog) is { } name ? new RenameResult(name, dialog.LeftChecked, dialog.RightChecked) : null;
    }

    public Task<string?> AskTextAsync(string title, string prompt, string initial) =>
        ShowAsync<string>(new InputDialog(title, prompt, initial));

    public async Task<AppSettings?> EditSettingsAsync(AppSettings current)
    {
        var copy = SettingsService.Clone(current);
        return await ShowAsync<bool>(new SettingsDialog(copy, _shell.TrashName)) ? copy : null;
    }

    public async Task<bool> ConfirmAsync(string title, string message, bool warning = false) =>
        await ShowAsync<MessageResult>(MessageDialog.Create(title, message, warning ? MessageKind.Warning : MessageKind.Question,
                                                            MessageButtons.YesNo, MessageResult.No)) == MessageResult.Yes;

    public Task ShowInfoAsync(string title, string message) =>
        ShowAsync<MessageResult>(MessageDialog.Create(title, message, MessageKind.Info));

    public Task ShowErrorAsync(string title, string message) =>
        ShowAsync<MessageResult>(MessageDialog.Create(title, message, MessageKind.Error));
}
