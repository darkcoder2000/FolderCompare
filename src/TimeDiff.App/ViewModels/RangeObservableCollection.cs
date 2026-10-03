using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;

namespace TimeDiff.App.ViewModels;

/// <summary>ObservableCollection with bulk operations that raise a single Reset notification.</summary>
public sealed class RangeObservableCollection<T> : ObservableCollection<T>
{
    public void ReplaceAll(IEnumerable<T> items)
    {
        Items.Clear();
        foreach (var item in items) Items.Add(item);
        RaiseReset();
    }

    public void InsertRange(int index, IReadOnlyList<T> items)
    {
        for (int i = 0; i < items.Count; i++) Items.Insert(index + i, items[i]);
        RaiseReset();
    }

    public void RemoveRange(int index, int count)
    {
        if (Items is List<T> list) list.RemoveRange(index, count);
        else for (int i = 0; i < count; i++) Items.RemoveAt(index);
        RaiseReset();
    }

    private void RaiseReset()
    {
        OnPropertyChanged(new PropertyChangedEventArgs(nameof(Count)));
        OnPropertyChanged(new PropertyChangedEventArgs("Item[]"));
        OnCollectionChanged(new NotifyCollectionChangedEventArgs(NotifyCollectionChangedAction.Reset));
    }
}
