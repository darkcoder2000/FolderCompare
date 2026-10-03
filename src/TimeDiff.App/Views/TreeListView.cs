using System.Collections;
using System.Windows.Controls;

namespace TimeDiff.App.Views;

/// <summary>ListView that exposes bulk selection (fast even for 100k rows).</summary>
public sealed class TreeListView : ListView
{
    public void SelectItems(IEnumerable items) => SetSelectedItems(items);
}
