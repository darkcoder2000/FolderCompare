using Avalonia.Controls;
using Avalonia.Input.Platform;
using Avalonia.Interactivity;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

public partial class SummaryDialog : Window
{
    public SummaryDialog() => InitializeComponent();

    public SummaryDialog(string title, OperationSummary summary, IReadOnlyList<string> notes) : this()
    {
        Title = title + " - summary";
        HeaderText.Text = $"{summary.Succeeded:N0} succeeded, {summary.Skipped:N0} skipped, {summary.Failed:N0} failed" +
                          (summary.Cancelled ? " - cancelled" : "");
        var report = summary.ToReport();
        if (notes.Count > 0) report += Environment.NewLine + "Not included:" + Environment.NewLine + string.Join(Environment.NewLine, notes.Select(n => "  " + n));
        ReportText.Text = report;
    }

    private async void Copy_Click(object? sender, RoutedEventArgs e)
    {
        try
        {
            if (Clipboard is { } clipboard) await clipboard.SetTextAsync(ReportText.Text ?? "");
        }
        catch (Exception ex)
        {
            await MessageDialog.Create(Title ?? "", ex.Message, MessageKind.Warning).ShowDialog(this);
        }
    }

    private void Close_Click(object? sender, RoutedEventArgs e) => Close();
}
