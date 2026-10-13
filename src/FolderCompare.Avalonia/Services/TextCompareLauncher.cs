using System.IO.Abstractions;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using FolderCompare.App.Views.TextCompare;
using FolderCompare.Core.TextDiff;
using Microsoft.Extensions.Logging;

namespace FolderCompare.App.Services;

public sealed class TextCompareLauncher : ITextCompareLauncher
{
    private const long LargeFileBytes = 20L * 1024 * 1024;

    private readonly IFileSystem _fs;
    private readonly SettingsService _settings;
    private readonly IDialogService _dialogs;
    private readonly ILogger<TextCompareLauncher> _logger;

    public TextCompareLauncher(IFileSystem fs, SettingsService settings, IDialogService dialogs, ILogger<TextCompareLauncher> logger)
    {
        _fs = fs;
        _settings = settings;
        _dialogs = dialogs;
        _logger = logger;
    }

    public async Task OpenAsync(TextCompareRequest request)
    {
        foreach (var (path, exists) in new[] { (request.LeftPath, request.LeftExists), (request.RightPath, request.RightExists) })
        {
            if (exists && _fs.FileInfo.New(path).Length > LargeFileBytes &&
                !await _dialogs.ConfirmAsync("Compare contents", $"{path}\n\nis larger than 20 MB. Comparing it may be slow. Continue?"))
                return;
        }

        var window = CreateWindow(request, out var error);
        if (window is null)
        {
            await _dialogs.ShowInfoAsync("Compare contents", error);
            return;
        }
        window.Show();
    }

    /// <summary>Loads both files and creates the window (not shown yet); null with an error text when loading failed.</summary>
    public Window? CreateWindow(TextCompareRequest request, out string error)
    {
        if (!TryLoad(request, out var left, out var right, out error)) return null;
        _logger.LogInformation("Opening text compare for {Left} and {Right}", request.LeftPath, request.RightPath);
        return new TextCompareWindow(this, _fs, _settings, _logger, request, left, right);
    }

    public async Task<bool> ConfirmCloseAllAsync()
    {
        var windows = Application.Current?.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop
            ? desktop.Windows.OfType<TextCompareWindow>().ToList()
            : [];
        foreach (var w in windows)
            if (!await w.ConfirmCloseAsync()) return false;
        foreach (var w in windows) w.CloseWithoutPrompt();
        return true;
    }

    /// <summary>Loads both sides. A missing side gets the other side's encoding and line endings.</summary>
    internal bool TryLoad(TextCompareRequest request, out TextSide left, out TextSide right, out string error)
    {
        left = right = null!;
        try
        {
            var l = request.LeftExists ? TextFileCodec.Load(_fs, request.LeftPath) : null;
            var r = request.RightExists ? TextFileCodec.Load(_fs, request.RightPath) : null;
            var binary = l?.IsBinary == true ? request.LeftPath : r?.IsBinary == true ? request.RightPath : null;
            if (binary is not null)
            {
                error = $"{binary}\n\nis not a text file and cannot be compared as text.";
                return false;
            }

            left = Side(request.LeftPath, l, r);
            right = Side(request.RightPath, r, l);
            error = "";
            return true;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            _logger.LogWarning(ex, "Could not load files for text compare");
            error = ex.Message;
            return false;
        }
    }

    private TextSide Side(string path, LoadedText? text, LoadedText? other)
    {
        if (text is not null) return new TextSide(path, true, text, _fs.File.GetLastWriteTimeUtc(path));
        var template = other ?? LoadedText.Empty;
        return new TextSide(path, false, template with { Text = "" }, null);
    }
}
