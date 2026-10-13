using System.IO.Abstractions;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging;
using Serilog;
using FolderCompare.App.Services;
using FolderCompare.App.ViewModels;
using FolderCompare.App.Views;
using FolderCompare.Core.Comparison;
using FolderCompare.Core.Operations;
using FolderCompare.Core.Shell;

namespace FolderCompare.App;

public partial class App : Application
{
    private ServiceProvider? _services;

    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
            Start(desktop);
        base.OnFrameworkInitializationCompleted();
    }

    private void Start(IClassicDesktopStyleApplicationLifetime desktop)
    {
        Log.Logger = new LoggerConfiguration()
            .MinimumLevel.Information()
            .WriteTo.File(Path.Combine(AppPaths.LogDirectory, "foldercompare-.log"),
                          rollingInterval: RollingInterval.Day,
                          retainedFileCountLimit: 14)
            .CreateLogger();

        Dispatcher.UIThread.UnhandledException += OnUnhandledUiException;
        AppDomain.CurrentDomain.UnhandledException += (_, args) =>
            Log.Fatal(args.ExceptionObject as Exception, "Unhandled exception");
        TaskScheduler.UnobservedTaskException += (_, args) =>
        {
            Log.Error(args.Exception, "Unobserved task exception");
            args.SetObserved();
        };
        desktop.Exit += (_, _) =>
        {
            Log.Information("FolderCompare exiting");
            _services?.Dispose();
            Log.CloseAndFlush();
        };

        var services = new ServiceCollection();
        services.AddLogging(b => b.AddSerilog(dispose: true));
        services.AddSingleton<IFileSystem, FileSystem>();
        services.AddSingleton<DiffEngine>();
        services.AddSingleton(sp => RecycleBins.ForCurrentOS(sp.GetRequiredService<IFileSystem>(), sp.GetRequiredService<ILoggerFactory>()));
        services.AddSingleton<FileOperationExecutor>();
        services.AddSingleton<SettingsService>();
        services.AddSingleton<IDialogService, DialogService>();
        services.AddSingleton(_ => ShellService.ForCurrentOS());
        services.AddSingleton<TextCompareLauncher>();
        services.AddSingleton<ITextCompareLauncher>(sp => sp.GetRequiredService<TextCompareLauncher>());
        services.AddSingleton<IExplorerIntegrationService>(sp => OperatingSystem.IsWindows()
            ? new ExplorerIntegrationService(sp.GetRequiredService<ILogger<ExplorerIntegrationService>>())
            : new NoExplorerIntegration());
        services.AddSingleton<MainViewModel>();
        services.AddSingleton<MainWindow>();
        _services = services.BuildServiceProvider();

        var args = desktop.Args ?? [];
        Log.Information("FolderCompare {Version} started on {OS} with arguments {Args}",
            typeof(App).Assembly.GetName().Version, System.Runtime.InteropServices.RuntimeInformation.OSDescription, args);
        _services.GetRequiredService<IExplorerIntegrationService>().EnsureCurrentPath();

        var options = CommandLineOptions.Parse(args);
        if (options.Left is not null && options.Right is not null
            && ExplorerMenuLayout.Classify(_services.GetRequiredService<IFileSystem>(), options.Left, options.Right) == PairKind.BothFiles)
        {
            OpenTextCompareOnly(desktop, Path.GetFullPath(options.Left), Path.GetFullPath(options.Right));
            return;
        }

        try
        {
            _services.GetRequiredService<MainViewModel>().ApplyCommandLine(options);
            desktop.ShutdownMode = ShutdownMode.OnMainWindowClose;
            desktop.MainWindow = _services.GetRequiredService<MainWindow>();
        }
        catch (Exception ex)
        {
            Log.Fatal(ex, "Startup failed");
            var error = MessageDialog.Create("FolderCompare", $"FolderCompare could not start:\n\n{ex.Message}", MessageKind.Error);
            error.Closed += (_, _) => desktop.Shutdown(1);
            desktop.MainWindow = error;
        }
    }

    /// <summary>Two files on the command line (e.g. Explorer "Compare Files"): only the text compare window opens.</summary>
    private void OpenTextCompareOnly(IClassicDesktopStyleApplicationLifetime desktop, string left, string right)
    {
        Log.Information("Comparing files {Left} and {Right} from the command line", left, right);
        desktop.ShutdownMode = ShutdownMode.OnLastWindowClose;
        var leftName = Path.GetFileName(left);
        var rightName = Path.GetFileName(right);
        var title = string.Equals(leftName, rightName, StringComparison.OrdinalIgnoreCase) ? rightName : $"{leftName} / {rightName}";
        var window = _services!.GetRequiredService<TextCompareLauncher>()
            .CreateWindow(new TextCompareRequest(left, true, right, true, title, null), out var error);
        desktop.MainWindow = window ?? MessageDialog.Create("Compare contents", error, MessageKind.Info);
    }

    private void OnUnhandledUiException(object? sender, DispatcherUnhandledExceptionEventArgs e)
    {
        Log.Error(e.Exception, "Unhandled UI exception");
        e.Handled = true;
        var owner = ShellService.ActiveWindow();
        var message = $"Something went wrong:\n\n{e.Exception.Message}\n\nThe details were written to the log folder:\n{AppPaths.LogDirectory}";
        var dialog = MessageDialog.Create("FolderCompare", message, MessageKind.Error);
        if (owner is { IsVisible: true }) _ = dialog.ShowDialog(owner);
        else dialog.Show();
    }
}
