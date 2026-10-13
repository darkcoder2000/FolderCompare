using Avalonia.Controls;
using Avalonia.Interactivity;
using FolderCompare.App.Services;

namespace FolderCompare.App.Views;

/// <summary>
/// Single-line text prompt; also used for Rename (with left/right side choices). The dialog result is the
/// entered text, or null when cancelled.
/// </summary>
public partial class InputDialog : Window
{
    private readonly bool _isRename;

    public InputDialog() => InitializeComponent();

    public InputDialog(string title, string prompt, string initial) : this()
    {
        Title = title;
        PromptText.Text = prompt;
        ValueBox.Text = initial;
        Opened += (_, _) =>
        {
            ValueBox.Focus();
            var dot = _isRename ? initial.LastIndexOf('.') : -1;
            ValueBox.SelectionStart = 0;
            ValueBox.SelectionEnd = dot > 0 ? dot : initial.Length;
        };
    }

    public InputDialog(RenameRequest request) : this("Rename", "New name:", request.CurrentName)
    {
        _isRename = true;
        SidePanel.IsVisible = true;
        LeftCheck.IsEnabled = request.ExistsLeft;
        LeftCheck.IsChecked = request.ExistsLeft;
        RightCheck.IsEnabled = request.ExistsRight;
        RightCheck.IsChecked = request.ExistsRight;
    }

    public string Value => (ValueBox.Text ?? "").Trim();
    public bool LeftChecked => LeftCheck.IsChecked == true;
    public bool RightChecked => RightCheck.IsChecked == true;

    private void Ok_Click(object? sender, RoutedEventArgs e)
    {
        if (Value.Length == 0) return;
        if (_isRename)
        {
            if (Value.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0 || Value is "." or "..")
            {
                ErrorText.Text = "The name contains invalid characters.";
                ErrorText.IsVisible = true;
                return;
            }
            if (!LeftChecked && !RightChecked) return;
        }
        Close(Value);
    }

    private void Cancel_Click(object? sender, RoutedEventArgs e) => Close(null);
}
