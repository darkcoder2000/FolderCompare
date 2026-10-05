using FolderCompare.Core.Operations;

namespace FolderCompare.App.Services;

public sealed record PreviewRequest(string Title, OperationPlan Plan, bool IsDelete, bool Permanent, bool OfferDontAskAgain);

public sealed record PreviewResult(bool Confirmed, bool DontAskAgain);

public sealed record RenameRequest(string CurrentName, bool ExistsLeft, bool ExistsRight);

public sealed record RenameResult(string NewName, bool Left, bool Right);

public interface IDialogService
{
    string? PickFolder(string? initialPath);
    string? PickSaveFile(string defaultName, string filter);
    PreviewResult ShowPreview(PreviewRequest request);
    OverwriteDecision AskOverwrite(OverwriteConflict conflict);
    void ShowSummary(string title, OperationSummary summary, IReadOnlyList<string> notes);
    Sides AskDeleteSide();
    RenameResult? AskRename(RenameRequest request);
    string? AskText(string title, string prompt, string initial);
    AppSettings? EditSettings(AppSettings current);
    bool Confirm(string title, string message, bool warning = false);
    void ShowInfo(string title, string message);
    void ShowError(string title, string message);
}
