using System.IO;
using System.IO.Abstractions;
using System.Windows;
using FolderCompare.App.Views.TextCompare;
using FolderCompare.Core.TextDiff;
using Microsoft.Extensions.Logging;

namespace FolderCompare.App.Services;

/// <summary>A file pair to open in the text compare window. A side that does not exist yet starts empty.</summary>
public sealed record TextCompareRequest(
    string LeftPath, bool LeftExists,
    string RightPath, bool RightExists,
    string Title,
    Action? FileSaved);

/// <summary>One side as loaded from disk.</summary>
public sealed record TextSide(string Path, bool Exists, LoadedText Text, DateTime? LastWriteUtc);

public interface ITextCompareLauncher
{
    void Open(TextCompareRequest request);

    /// <summary>Asks every open compare window about unsaved changes. False when the user cancelled.</summary>
    bool ConfirmCloseAll();
}

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

    public void Open(TextCompareRequest request)
    {
        foreach (var (path, exists) in new[] { (request.LeftPath, request.LeftExists), (request.RightPath, request.RightExists) })
        {
            if (exists && _fs.FileInfo.New(path).Length > LargeFileBytes &&
                !_dialogs.Confirm("Compare contents", $"{path}\n\nis larger than 20 MB. Comparing it may be slow. Continue?"))
                return;
        }

        if (!TryLoad(request, out var left, out var right, out var error))
        {
            _dialogs.ShowInfo("Compare contents", error);
            return;
        }

        _logger.LogInformation("Opening text compare for {Left} and {Right}", request.LeftPath, request.RightPath);
        var window = new TextCompareWindow(this, _fs, _settings, _logger, request, left, right);
        window.Show();
    }

    public bool ConfirmCloseAll()
    {
        var windows = Application.Current.Windows.OfType<TextCompareWindow>().ToList();
        if (windows.Any(w => !w.ConfirmClose())) return false;
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
