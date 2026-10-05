using System.Windows;
using FolderCompare.Core.Operations;

namespace FolderCompare.App.Views;

public partial class DeleteSideDialog : Window
{
    public DeleteSideDialog() => InitializeComponent();

    public Sides Sides { get; private set; } = Sides.None;

    private void Finish(Sides sides)
    {
        Sides = sides;
        DialogResult = true;
    }

    private void Left_Click(object sender, RoutedEventArgs e) => Finish(Sides.Left);
    private void Right_Click(object sender, RoutedEventArgs e) => Finish(Sides.Right);
    private void Both_Click(object sender, RoutedEventArgs e) => Finish(Sides.Both);
}
