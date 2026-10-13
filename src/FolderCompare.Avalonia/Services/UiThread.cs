#if AVALONIA
using Avalonia.Threading;
#else
using System.Windows;
#endif

namespace FolderCompare.App.Services;

/// <summary>The few dispatcher calls the shared view models need, for both UI frameworks.</summary>
public static class UiThread
{
    /// <summary>Runs <paramref name="action"/> on the UI thread and returns its result.</summary>
    public static Task<T> InvokeAsync<T>(Func<Task<T>> action) =>
#if AVALONIA
        Dispatcher.UIThread.InvokeAsync(action);
#else
        Application.Current.Dispatcher.InvokeAsync(action).Task.Unwrap();
#endif

    /// <summary>Queues <paramref name="action"/> to run on the UI thread after the current work.</summary>
    public static void Post(Action action) =>
#if AVALONIA
        Dispatcher.UIThread.Post(action);
#else
        Application.Current.Dispatcher.BeginInvoke(action);
#endif
}
