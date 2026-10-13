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
        services.AddSingleton<IExplorerIntegrationService, ExplorerIntegrationService>();
        services.AddSingleton<MainViewModel>();
        services.AddSingleton<MainWindow>();
        _services = services.BuildServiceProvider();

        Log.Information("FolderCompare started with arguments {Args}", e.Args);
        _services.GetRequiredService<IExplorerIntegrationService>().EnsureCurrentPath();
        var options = CommandLineOptions.Parse(e.Args);
        if (options.Left is not null && options.Right is not null
            && ExplorerMenuLayout.Classify(_services.GetRequiredService<IFileSystem>(), options.Left, options.Right) == PairKind.BothFiles)
        {
            OpenTextCompare(Path.GetFullPath(options.Left), Path.GetFullPath(options.Right));
            return;
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

    /// <summary>Two files on the command line (e.g. Explorer "Compare Files"): only the text compare window opens.</summary>
    private void OpenTextCompare(string left, string right)
    {
        Log.Information("Comparing files {Left} and {Right} from the command line", left, right);
        ShutdownMode = ShutdownMode.OnLastWindowClose;
        var leftName = Path.GetFileName(left);
        var rightName = Path.GetFileName(right);
        var title = string.Equals(leftName, rightName, StringComparison.OrdinalIgnoreCase) ? rightName : $"{leftName} / {rightName}";
        _services!.GetRequiredService<ITextCompareLauncher>().Open(new TextCompareRequest(left, true, right, true, title, null));
        if (Windows.Count == 0) Shutdown(0);
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
