using Avalonia.Controls;
using Avalonia.Interactivity;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

/// <summary>The dialog result is the chosen <see cref="Sides"/> (None when cancelled).</summary>
public partial class DeleteSideDialog : Window
{
    public DeleteSideDialog()
    {
        InitializeComponent();
        Opened += (_, _) => CancelButton.Focus();
    }

    private void Left_Click(object? sender, RoutedEventArgs e) => Close(Sides.Left);
    private void Right_Click(object? sender, RoutedEventArgs e) => Close(Sides.Right);
    private void Both_Click(object? sender, RoutedEventArgs e) => Close(Sides.Both);
    private void Cancel_Click(object? sender, RoutedEventArgs e) => Close(Sides.None);
}
