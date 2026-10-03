using System.IO;
using System.Windows;
using TimeDiff.App.Services;

namespace TimeDiff.App.Views;

/// <summary>Single-line text prompt; also used for Rename (with left/right side choices).</summary>
public partial class InputDialog : Window
{
    private readonly bool _isRename;

    public InputDialog(string title, string prompt, string initial)
    {
        InitializeComponent();
        Title = title;
        PromptText.Text = prompt;
        ValueBox.Text = initial;
        Loaded += (_, _) =>
        {
            ValueBox.Focus();
            var dot = _isRename ? initial.LastIndexOf('.') : -1;
            if (dot > 0) ValueBox.Select(0, dot);
            else ValueBox.SelectAll();
        };
    }

    public InputDialog(RenameRequest request) : this("Rename", "New name:", request.CurrentName)
    {
        _isRename = true;
        SidePanel.Visibility = Visibility.Visible;
        LeftCheck.IsEnabled = request.ExistsLeft;
        LeftCheck.IsChecked = request.ExistsLeft;
        RightCheck.IsEnabled = request.ExistsRight;
        RightCheck.IsChecked = request.ExistsRight;
    }

    public string Value => ValueBox.Text.Trim();
    public bool LeftChecked => LeftCheck.IsChecked == true;
    public bool RightChecked => RightCheck.IsChecked == true;

    private void Ok_Click(object sender, RoutedEventArgs e)
    {
        if (Value.Length == 0) return;
        if (_isRename)
        {
            if (Value.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0)
            {
                MessageBox.Show(this, "The name contains invalid characters.", Title, MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }
            if (!LeftChecked && !RightChecked) return;
        }
        DialogResult = true;
    }
}
