using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Media;

namespace FolderCompare.App.Views;

public enum MessageKind
{
    Info,
    Question,
    Warning,
    Error,
}

public enum MessageButtons
{
    Ok,
    YesNo,
    YesNoCancel,
}

public enum MessageResult
{
    None,
    Ok,
    Yes,
    No,
    Cancel,
}

/// <summary>A simple message box (Avalonia has none). The dialog result is a <see cref="MessageResult"/>.</summary>
public partial class MessageDialog : Window
{
    public MessageDialog() => InitializeComponent();

    /// <param name="defaultResult">The button that Enter presses; Escape always means Cancel/No/OK.</param>
    public static MessageDialog Create(string title, string message, MessageKind kind,
                                       MessageButtons buttons = MessageButtons.Ok, MessageResult defaultResult = MessageResult.Ok)
    {
        var dialog = new MessageDialog { Title = title };
        dialog.MessageText.Text = message;
        var (iconKey, brushKey) = kind switch
        {
            MessageKind.Error => ("IconWarning", "ErrorBrush"),
            MessageKind.Warning => ("IconWarning", "ConflictBrush"),
            MessageKind.Question => ("IconFile", "OnlyBrush"),
            _ => ("IconFile", "OnlyBrush"),
        };
        dialog.KindIcon.Data = Resource<Geometry>(dialog, iconKey);
        dialog.KindIcon.Foreground = Resource<IBrush>(dialog, brushKey);
        dialog.KindIcon.IsVisible = kind is MessageKind.Error or MessageKind.Warning;

        var results = buttons switch
        {
            MessageButtons.YesNo => new[] { MessageResult.Yes, MessageResult.No },
            MessageButtons.YesNoCancel => new[] { MessageResult.Yes, MessageResult.No, MessageResult.Cancel },
            _ => new[] { MessageResult.Ok },
        };
        var cancel = results.Contains(MessageResult.Cancel) ? MessageResult.Cancel : results[^1];
        foreach (var result in results)
        {
            var button = new Button
            {
                Content = result switch
                {
                    MessageResult.Yes => "_Yes",
                    MessageResult.No => "_No",
                    MessageResult.Cancel => "_Cancel",
                    _ => "_OK",
                },
                IsDefault = result == defaultResult,
                IsCancel = result == cancel,
                Classes = { "dialog" },
            };
            button.Click += (_, _) => dialog.Close(result);
            dialog.Buttons.Children.Add(button);
        }
        dialog.Opened += (_, _) => dialog.Buttons.Children.OfType<Button>().FirstOrDefault(b => b.IsDefault)?.Focus();
        return dialog;
    }

    private static T? Resource<T>(Window window, string key) where T : class =>
        window.TryFindResource(key, window.ActualThemeVariant, out var value) ? value as T : null;
}
