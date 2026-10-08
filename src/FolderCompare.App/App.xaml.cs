using System.IO;
using System.IO.Abstractions;
using System.Windows;
using System.Windows.Threading;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging;
using Serilog;
using FolderCompare.App.Services;
using FolderCompare.App.ViewModels;
using FolderCompare.Core.Comparison;
using FolderCompare.Core.Operations;
using FolderCompare.Core.Shell;

namespace FolderCompare.App;

public partial class App : Application
{
    private ServiceProvider? _services;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        Log.Logger = new LoggerConfiguration()
            .MinimumLevel.Information()
            .WriteTo.File(Path.Combine(ShellService.LogDirectory, "foldercompare-.log"),
                          rollingInterval: RollingInterval.Day,
                          retainedFileCountLimit: 14)
            .CreateLogger();

        DispatcherUnhandledException += OnDispatcherUnhandledException;
        AppDomain.CurrentDomain.UnhandledException += (_, args) =>
            Log.Fatal(args.ExceptionObject as Exception, "Unhandled exception");
        TaskScheduler.UnobservedTaskException += (_, args) =>
        {
            Log.Error(args.Exception, "Unobserved task exception");
            args.SetObserved();
        };

        var services = new ServiceCollection();
        services.AddLogging(b => b.AddSerilog(dispose: true));
        services.AddSingleton<IFileSystem, FileSystem>();
        services.AddSingleton<DiffEngine>();
        services.AddSingleton<IRecycleBin, RecycleBin>();
        services.AddSingleton<FileOperationExecutor>();
        services.AddSingleton<SettingsService>();
        services.AddSingleton<IDialogService, DialogService>();
        services.AddSingleton<IShellService, ShellService>();
        services.AddSingleton<ITextCompareLauncher, TextCompareLauncher>();
        services.AddSingleton(sp => new ExplorerSelectionStore(sp.GetRequiredService<IFileSystem>(), SettingsService.SettingsDirectory));
        services.AddSingleton<IExplorerIntegrationService, ExplorerIntegrationService>();
        services.AddSingleton<MainViewModel>();
        services.AddSingleton<MainWindow>();
        _services = services.BuildServiceProvider();

        Log.Information("FolderCompare started with arguments {Args}", e.Args);
        var options = CommandLineOptions.Parse(e.Args);
        if (options.SelectLeft is not null)
        {
            SelectLeft(options.SelectLeft);
            return;
        }
        if (options.CompareToLeft is not null)
        {
            var folders = CompareToLeft(options.CompareToLeft);
            if (folders is null) return;
            options = folders;
        }
        else
        {
            _services.GetRequiredService<IExplorerIntegrationService>().EnsureCurrentPath();
        }

        var viewModel = _services.GetRequiredService<MainViewModel>();
        viewModel.ApplyCommandLine(options);

        try
        {
            var window = _services.GetRequiredService<MainWindow>();
            MainWindow = window;
            window.Show();
        }
        catch (Exception ex)
        {
            Log.Fatal(ex, "Startup failed");
            MessageBox.Show($"FolderCompare could not start:\n\n{ex.Message}", "FolderCompare", MessageBoxButton.OK, MessageBoxImage.Error);
            Shutdown(1);
        }
    }

    /// <summary>Explorer "Select as left": remember the item and exit without showing a window.</summary>
    private void SelectLeft(string path)
    {
        try
        {
            var full = Path.GetFullPath(path);
            _services!.GetRequiredService<ExplorerSelectionStore>().Save(full);
            _services!.GetRequiredService<IExplorerIntegrationService>().UpdateLabel(full);
            Log.Information("Selected {Path} as left from Explorer", full);
        }
        catch (Exception ex)
        {
            Log.Error(ex, "Could not store the left selection {Path}", path);
            MessageBox.Show($"Could not remember the left item:\n\n{ex.Message}", "FolderCompare", MessageBoxButton.OK, MessageBoxImage.Error);
        }
        Shutdown(0);
    }

    /// <summary>
    /// Explorer "Compare to left". Two files open the text compare window directly. Two folders return the
    /// options for the main window; otherwise null (the app was shut down or only shows the text compare).
    /// </summary>
    private CommandLineOptions? CompareToLeft(string path)
    {
        var services = _services!;
        var store = services.GetRequiredService<ExplorerSelectionStore>();
        string? left = null;
        try
        {
            left = store.Read();
            store.Clear();
            services.GetRequiredService<IExplorerIntegrationService>().UpdateLabel(null);
        }
        catch (Exception ex)
        {
            Log.Warning(ex, "Could not read the left selection");
        }

        var right = Path.GetFullPath(path);
        if (left is null)
            return Refuse("No left item selected. Right-click a file or folder and choose \"Select as left for FolderCompare\" first.");

        Log.Information("Comparing {Left} with {Right} from Explorer", left, right);
        switch (ExplorerMenuLayout.Classify(services.GetRequiredService<IFileSystem>(), left, right))
        {
            case PairKind.BothFolders:
                return new CommandLineOptions(left, right, Compare: true, NoRecursive: false);
            case PairKind.BothFiles:
                ShutdownMode = ShutdownMode.OnLastWindowClose;
                var leftName = Path.GetFileName(left);
                var rightName = Path.GetFileName(right);
                var title = string.Equals(leftName, rightName, StringComparison.OrdinalIgnoreCase) ? rightName : $"{leftName} / {rightName}";
                services.GetRequiredService<ITextCompareLauncher>().Open(new TextCompareRequest(left, true, right, true, title, null));
                if (Windows.Count == 0) Shutdown(0);
                return null;
            case PairKind.Mixed:
                return Refuse($"A file can only be compared with a file, and a folder with a folder.\n\nLeft: {left}\nRight: {right}");
            default:
                return Refuse($"The left or right item no longer exists.\n\nLeft: {left}\nRight: {right}");
        }
    }

    private CommandLineOptions? Refuse(string message)
    {
        MessageBox.Show(message, "FolderCompare", MessageBoxButton.OK, MessageBoxImage.Information);
        Shutdown(0);
        return null;
    }

    protected override void OnExit(ExitEventArgs e)
    {
        Log.Information("FolderCompare exiting");
        _services?.Dispose();
        Log.CloseAndFlush();
        base.OnExit(e);
    }

    private void OnDispatcherUnhandledException(object sender, DispatcherUnhandledExceptionEventArgs e)
    {
        Log.Error(e.Exception, "Unhandled UI exception");
        MessageBox.Show(
            $"Something went wrong:\n\n{e.Exception.Message}\n\nThe details were written to the log folder:\n{ShellService.LogDirectory}",
            "FolderCompare", MessageBoxButton.OK, MessageBoxImage.Error);
        e.Handled = true;
    }
}
