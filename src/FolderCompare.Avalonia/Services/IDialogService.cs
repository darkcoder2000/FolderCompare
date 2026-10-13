using FolderCompare.Core.Operations;

namespace FolderCompare.App.Services;

public sealed record PreviewRequest(string Title, OperationPlan Plan, bool IsDelete, bool Permanent, bool OfferDontAskAgain);

public sealed record PreviewResult(bool Confirmed, bool DontAskAgain);

public sealed record RenameRequest(string CurrentName, bool ExistsLeft, bool ExistsRight);

public sealed record RenameResult(string NewName, bool Left, bool Right);

/// <summary>A file type offered by the save dialog, e.g. ("CSV file", "csv").</summary>
public sealed record FileType(string Name, string Extension);

/// <summary>
/// Modal dialogs. All methods are asynchronous because Avalonia dialogs are; each task completes when the dialog
/// is closed, so callers must await it before acting on the answer.
/// </summary>
public interface IDialogService
{
    Task<string?> PickFolderAsync(string? initialPath);
    Task<string?> PickSaveFileAsync(string defaultName, IReadOnlyList<FileType> types);
    Task<PreviewResult> ShowPreviewAsync(PreviewRequest request);
    Task<OverwriteDecision> AskOverwriteAsync(OverwriteConflict conflict);
    Task ShowSummaryAsync(string title, OperationSummary summary, IReadOnlyList<string> notes);
    Task<Sides> AskDeleteSideAsync();
    Task<RenameResult?> AskRenameAsync(RenameRequest request);
    Task<string?> AskTextAsync(string title, string prompt, string initial);
    Task<AppSettings?> EditSettingsAsync(AppSettings current);
    Task<bool> ConfirmAsync(string title, string message, bool warning = false);
    Task ShowInfoAsync(string title, string message);
    Task ShowErrorAsync(string title, string message);
}
