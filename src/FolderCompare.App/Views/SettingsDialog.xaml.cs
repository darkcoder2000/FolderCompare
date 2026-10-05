using System.Windows;
using System.Windows.Controls;
using FolderCompare.App.Services;
using FolderCompare.Core.Models;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

public partial class SettingsDialog : Window
{
    public SettingsDialog(AppSettings editable)
    {
        InitializeComponent();
        SymlinkCombo.ItemsSource = Enum.GetValues<SymlinkMode>();
        OverwriteCombo.ItemsSource = Enum.GetValues<OverwritePolicy>();
        DataContext = editable;
    }

    private void Ok_Click(object sender, RoutedEventArgs e)
    {
        if (HasErrors(this))
        {
            MessageBox.Show(this, "Please correct the highlighted values.", Title, MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }
        var s = (AppSettings)DataContext;
        if (s.ToleranceSeconds < 0 || s.PreviewThreshold < 0)
        {
            MessageBox.Show(this, "Values must not be negative.", Title, MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }
        DialogResult = true;
    }

    private static bool HasErrors(DependencyObject node)
    {
        if (Validation.GetHasError(node)) return true;
        foreach (var child in LogicalTreeHelper.GetChildren(node).OfType<DependencyObject>())
            if (HasErrors(child)) return true;
        return false;
    }
}
