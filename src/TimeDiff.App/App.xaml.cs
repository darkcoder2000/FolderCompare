using System.IO;
using System.IO.Abstractions;
using System.Windows;
using System.Windows.Threading;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging;
using Serilog;
using TimeDiff.App.Services;
using TimeDiff.App.ViewModels;
using TimeDiff.Core.Comparison;
using TimeDiff.Core.Operations;

namespace TimeDiff.App;

public partial class App : Application
{
    private ServiceProvider? _services;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        Log.Logger = new LoggerConfiguration()
            .MinimumLevel.Information()
            .WriteTo.File(Path.Combine(ShellService.LogDirectory, "timediff-.log"),
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
        services.AddSingleton<MainViewModel>();
        services.AddSingleton<MainWindow>();
        _services = services.BuildServiceProvider();

        Log.Information("TimeDiff started with arguments {Args}", e.Args);
        var viewModel = _services.GetRequiredService<MainViewModel>();
        viewModel.ApplyCommandLine(CommandLineOptions.Parse(e.Args));

        try
        {
            var window = _services.GetRequiredService<MainWindow>();
            MainWindow = window;
            window.Show();
        }
        catch (Exception ex)
        {
            Log.Fatal(ex, "Startup failed");
            MessageBox.Show($"TimeDiff could not start:\n\n{ex.Message}", "TimeDiff", MessageBoxButton.OK, MessageBoxImage.Error);
            Shutdown(1);
        }
    }

    protected override void OnExit(ExitEventArgs e)
    {
        Log.Information("TimeDiff exiting");
        _services?.Dispose();
        Log.CloseAndFlush();
        base.OnExit(e);
    }

    private void OnDispatcherUnhandledException(object sender, DispatcherUnhandledExceptionEventArgs e)
    {
        Log.Error(e.Exception, "Unhandled UI exception");
        MessageBox.Show(
            $"Something went wrong:\n\n{e.Exception.Message}\n\nThe details were written to the log folder:\n{ShellService.LogDirectory}",
            "TimeDiff", MessageBoxButton.OK, MessageBoxImage.Error);
        e.Handled = true;
    }
}
