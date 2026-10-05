using System.Windows;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

public partial class SummaryDialog : Window
{
    public SummaryDialog(string title, OperationSummary summary, IReadOnlyList<string> notes)
    {
        InitializeComponent();
        Title = title + " - summary";
        HeaderText.Text = $"{summary.Succeeded:N0} succeeded, {summary.Skipped:N0} skipped, {summary.Failed:N0} failed" +
                          (summary.Cancelled ? " - cancelled" : "");
        var report = summary.ToReport();
        if (notes.Count > 0) report += Environment.NewLine + "Not included:" + Environment.NewLine + string.Join(Environment.NewLine, notes.Select(n => "  " + n));
        ReportText.Text = report;
    }

    private void Copy_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            Clipboard.SetText(ReportText.Text);
        }
        catch (Exception ex)
        {
            MessageBox.Show(this, ex.Message, Title, MessageBoxButton.OK, MessageBoxImage.Warning);
        }
    }
}
