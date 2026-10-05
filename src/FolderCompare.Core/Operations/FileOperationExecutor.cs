using System.IO.Abstractions;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Logging.Abstractions;
using FolderCompare.Core.Comparison;
using FolderCompare.Core.Models;

namespace FolderCompare.Core.Operations;

/// <summary>
/// Executes planned actions one after another (an operation queue). Each item failure is recorded and the
/// queue continues; cancellation stops after cleaning up the current file so no partial files remain.
/// </summary>
public sealed class FileOperationExecutor
{
    public const string TempSuffix = ".tmpcopy";
    private const int BufferSize = 1 << 20;
    private const FileAttributes CopiedAttributes = FileAttributes.ReadOnly | FileAttributes.Hidden | FileAttributes.System | FileAttributes.Archive;

    private readonly IFileSystem _fs;
    private readonly IRecycleBin _recycleBin;
    private readonly ILogger _logger;

    public FileOperationExecutor(IFileSystem fs, IRecycleBin recycleBin, ILogger<FileOperationExecutor>? logger = null)
    {
        _fs = fs;
        _recycleBin = recycleBin;
        _logger = (ILogger?)logger ?? NullLogger.Instance;
    }

    public async Task<OperationSummary> ExecuteAsync(
        IReadOnlyList<PlannedAction> actions,
        OperationOptions options,
        OverwriteResolver? resolver,
        IProgress<OperationProgress>? progress = null,
        CancellationToken ct = default)
    {
        var summary = new OperationSummary();
        var state = new State(options.OverwritePolicy);
        long totalBytes = actions.Sum(a => a.Kind == ActionKind.CopyFile ? a.Size : 0);
        long bytesDone = 0;

        for (int i = 0; i < actions.Count; i++)
        {
            var action = actions[i];
            if (ct.IsCancellationRequested)
            {
                summary.Cancelled = true;
                for (int j = i; j < actions.Count; j++)
                    summary.Items.Add(new OperationItemResult(actions[j], ItemOutcome.Skipped, "Cancelled"));
                break;
            }

            progress?.Report(new OperationProgress(action.TargetPath, 0, action.Size, bytesDone, totalBytes, i, actions.Count));
            OperationItemResult result;
            try
            {
                long baseBytes = bytesDone;
                var fileProgress = progress is null ? null : new Action<long>(done =>
                    progress.Report(new OperationProgress(action.TargetPath, done, action.Size, baseBytes + done, totalBytes, i, actions.Count)));

                result = action.Kind switch
                {
                    ActionKind.CopyFile => await CopyFileAsync(action, options, state, resolver, fileProgress, ct).ConfigureAwait(false),
                    ActionKind.CreateDirectory => CreateDirectory(action, options),
                    ActionKind.Delete => Delete(action, options),
                    _ => throw new NotSupportedException(action.Kind.ToString()),
                };
            }
            catch (OperationCanceledException)
            {
                summary.Cancelled = true;
                for (int j = i; j < actions.Count; j++)
                    summary.Items.Add(new OperationItemResult(actions[j], ItemOutcome.Skipped, "Cancelled"));
                Log(actions[i], ItemOutcome.Skipped, "Cancelled");
                break;
            }
            catch (Exception ex) when (IsItemError(ex))
            {
                result = new OperationItemResult(action, ItemOutcome.Failed, ex.Message);
            }

            if (action.Kind == ActionKind.CopyFile) bytesDone += action.Size;
            summary.Items.Add(result);
            Log(action, result.Outcome, result.Message, result.FinalTargetPath);
        }

        progress?.Report(new OperationProgress("", 0, 0, bytesDone, totalBytes, actions.Count, actions.Count));
        return summary;
    }

    /// <summary>Renames an item on one side within its folder.</summary>
    public void Rename(OperationOptions options, Side side, string relativePath, string newName)
    {
        if (string.IsNullOrWhiteSpace(newName) || newName.IndexOfAny(_fs.Path.GetInvalidFileNameChars()) >= 0 || newName is "." or "..")
            throw new ArgumentException($"'{newName}' is not a valid name.");

        var root = options.RootFor(side);
        var source = SafetyValidator.EnsureInsideRoot(_fs, root, _fs.Path.Join(root, relativePath));
        var target = SafetyValidator.EnsureInsideRoot(_fs, root, _fs.Path.Join(_fs.Path.GetDirectoryName(source)!, newName));
        try
        {
            if (_fs.Directory.Exists(source)) _fs.Directory.Move(source, target);
            else _fs.File.Move(source, target);
            _logger.LogInformation("Rename {Source} -> {Target}: {Result}", source, target, ItemOutcome.Succeeded);
        }
        catch (Exception ex)
        {
            _logger.LogWarning("Rename {Source} -> {Target}: {Result} {Error}", source, target, ItemOutcome.Failed, ex.Message);
            throw;
        }
    }

    private async Task<OperationItemResult> CopyFileAsync(PlannedAction action, OperationOptions options, State state,
                                                          OverwriteResolver? resolver, Action<long>? fileProgress, CancellationToken ct)
    {
        var sourceSide = action.TargetSide == Side.Left ? Side.Right : Side.Left;
        var source = SafetyValidator.EnsureInsideRoot(_fs, options.RootFor(sourceSide), action.SourcePath!);
        var targetRoot = options.RootFor(action.TargetSide);
        var target = SafetyValidator.EnsureInsideRoot(_fs, targetRoot, action.TargetPath);

        var sourceInfo = _fs.FileInfo.New(source);
        if (!sourceInfo.Exists) return new OperationItemResult(action, ItemOutcome.Failed, "Source no longer exists.");
        if (_fs.Directory.Exists(target)) return new OperationItemResult(action, ItemOutcome.Failed, "A folder with the same name exists at the destination.");

        var targetInfo = _fs.FileInfo.New(target);
        if (targetInfo.Exists)
        {
            bool targetNewer = TimestampComparer.IsNewer(targetInfo.LastWriteTimeUtc, sourceInfo.LastWriteTimeUtc, options.Tolerance);
            var choice = ToChoice(state.Policy);
            bool mustAsk = state.Policy == OverwritePolicy.Ask ||
                           (targetNewer && choice == OverwriteChoice.Overwrite && !state.NewerOverwriteConfirmed);
            if (mustAsk)
            {
                if (resolver is null) return new OperationItemResult(action, ItemOutcome.Skipped, "Destination exists.");
                var conflict = new OverwriteConflict(source, target, sourceInfo.Length, sourceInfo.LastWriteTimeUtc,
                                                     targetInfo.Length, targetInfo.LastWriteTimeUtc, targetNewer);
                var decision = await resolver(conflict, ct).ConfigureAwait(false);
                if (decision.Choice == OverwriteChoice.Cancel) throw new OperationCanceledException();
                choice = decision.Choice;
                if (decision.ApplyToAll)
                {
                    if (state.Policy == OverwritePolicy.Ask) state.Policy = ToPolicy(choice);
                    if (targetNewer && choice == OverwriteChoice.Overwrite) state.NewerOverwriteConfirmed = true;
                }
            }

            switch (choice)
            {
                case OverwriteChoice.Skip:
                    return new OperationItemResult(action, ItemOutcome.Skipped, "Destination exists.");
                case OverwriteChoice.OverwriteIfNewer when !TimestampComparer.IsNewer(sourceInfo.LastWriteTimeUtc, targetInfo.LastWriteTimeUtc, options.Tolerance):
                    return new OperationItemResult(action, ItemOutcome.Skipped, "Destination is not older than the source.");
                case OverwriteChoice.KeepBoth:
                    target = SafetyValidator.EnsureInsideRoot(_fs, targetRoot, UniqueName(target));
                    break;
            }
        }

        var directory = _fs.Path.GetDirectoryName(target)!;
        if (!string.Equals(_fs.Path.TrimEndingDirectorySeparator(directory), SafetyValidator.Normalize(_fs, targetRoot), StringComparison.OrdinalIgnoreCase))
            SafetyValidator.EnsureInsideRoot(_fs, targetRoot, directory);
        _fs.Directory.CreateDirectory(directory);

        var temp = target + TempSuffix;
        if (_fs.File.Exists(temp) || _fs.Directory.Exists(temp))
            temp = $"{target}.{Guid.NewGuid().ToString("N")[..8]}{TempSuffix}";

        try
        {
            await CopyContentAsync(source, temp, sourceInfo.Length, fileProgress, ct).ConfigureAwait(false);
            _fs.File.SetLastWriteTimeUtc(temp, sourceInfo.LastWriteTimeUtc);
            try
            {
                _fs.File.SetCreationTimeUtc(temp, sourceInfo.CreationTimeUtc);
            }
            catch (Exception ex) when (IsItemError(ex))
            {
                // Creation time is best effort.
            }

            if (_fs.File.Exists(target))
            {
                var attributes = _fs.File.GetAttributes(target);
                if (attributes.HasFlag(FileAttributes.ReadOnly))
                    _fs.File.SetAttributes(target, attributes & ~FileAttributes.ReadOnly);
            }
            _fs.File.Move(temp, target, overwrite: true);
        }
        catch
        {
            TryDeleteFile(temp);
            throw;
        }

        var wanted = sourceInfo.Attributes & CopiedAttributes;
        if (wanted != 0) _fs.File.SetAttributes(target, wanted);

        // Verify the result.
        var check = _fs.FileInfo.New(target);
        check.Refresh();
        if (!check.Exists) return new OperationItemResult(action, ItemOutcome.Failed, "Verification failed: destination does not exist.", target);
        if (check.Length != sourceInfo.Length)
            return new OperationItemResult(action, ItemOutcome.Failed, $"Verification failed: size {check.Length} differs from source size {sourceInfo.Length}.", target);
        if ((check.LastWriteTimeUtc - sourceInfo.LastWriteTimeUtc).Duration() > options.Tolerance)
            return new OperationItemResult(action, ItemOutcome.Failed, "Verification failed: timestamp was not preserved.", target);

        return new OperationItemResult(action, ItemOutcome.Succeeded, null, target);
    }

    private async Task CopyContentAsync(string source, string temp, long length, Action<long>? fileProgress, CancellationToken ct)
    {
        await using var input = _fs.FileStream.New(source, FileMode.Open, FileAccess.Read, FileShare.Read, BufferSize, FileOptions.SequentialScan | FileOptions.Asynchronous);
        await using var output = _fs.FileStream.New(temp, FileMode.CreateNew, FileAccess.Write, FileShare.None, BufferSize, FileOptions.Asynchronous);
        if (length > 0) output.SetLength(length);

        var buffer = new byte[Math.Min(BufferSize, Math.Max(length, 1))];
        long done = 0;
        long lastReport = 0;
        int read;
        while ((read = await input.ReadAsync(buffer, ct).ConfigureAwait(false)) > 0)
        {
            await output.WriteAsync(buffer.AsMemory(0, read), ct).ConfigureAwait(false);
            done += read;
            if (fileProgress is not null && done - lastReport >= 8 * BufferSize)
            {
                lastReport = done;
                fileProgress(done);
            }
        }
        if (output.Length != done) output.SetLength(done);
        fileProgress?.Invoke(done);
    }

    private OperationItemResult CreateDirectory(PlannedAction action, OperationOptions options)
    {
        var target = SafetyValidator.EnsureInsideRoot(_fs, options.RootFor(action.TargetSide), action.TargetPath);
        if (_fs.File.Exists(target)) return new OperationItemResult(action, ItemOutcome.Failed, "A file with the same name exists at the destination.");
        _fs.Directory.CreateDirectory(target);
        return new OperationItemResult(action, ItemOutcome.Succeeded, null, target);
    }

    private OperationItemResult Delete(PlannedAction action, OperationOptions options)
    {
        var target = SafetyValidator.EnsureInsideRoot(_fs, options.RootFor(action.TargetSide), action.TargetPath);
        if (action.IsDirectory)
        {
            if (!_fs.Directory.Exists(target)) return new OperationItemResult(action, ItemOutcome.Skipped, "Folder no longer exists.");
            if (options.DeleteToRecycleBin)
            {
                _recycleBin.DeleteDirectory(target);
            }
            else
            {
                ClearReadOnly(target);
                _fs.Directory.Delete(target, recursive: true);
            }
        }
        else
        {
            if (!_fs.File.Exists(target)) return new OperationItemResult(action, ItemOutcome.Skipped, "File no longer exists.");
            if (options.DeleteToRecycleBin)
            {
                _recycleBin.DeleteFile(target);
            }
            else
            {
                var attributes = _fs.File.GetAttributes(target);
                if (attributes.HasFlag(FileAttributes.ReadOnly)) _fs.File.SetAttributes(target, attributes & ~FileAttributes.ReadOnly);
                _fs.File.Delete(target);
            }
        }
        return new OperationItemResult(action, ItemOutcome.Succeeded, null, target);
    }

    private void ClearReadOnly(string directory)
    {
        var info = _fs.DirectoryInfo.New(directory);
        if (info.LinkTarget is not null) return; // never follow links while deleting
        foreach (var file in info.EnumerateFiles("*", SearchOption.TopDirectoryOnly))
        {
            if (file.Attributes.HasFlag(FileAttributes.ReadOnly)) file.Attributes &= ~FileAttributes.ReadOnly;
        }
        foreach (var sub in info.EnumerateDirectories("*", SearchOption.TopDirectoryOnly))
            ClearReadOnly(sub.FullName);
    }

    private string UniqueName(string path)
    {
        var dir = _fs.Path.GetDirectoryName(path)!;
        var stem = _fs.Path.GetFileNameWithoutExtension(path);
        var ext = _fs.Path.GetExtension(path);
        for (int i = 1; ; i++)
        {
            var candidate = _fs.Path.Join(dir, $"{stem} ({i}){ext}");
            if (!_fs.File.Exists(candidate) && !_fs.Directory.Exists(candidate)) return candidate;
        }
    }

    private void TryDeleteFile(string path)
    {
        try
        {
            if (_fs.File.Exists(path)) _fs.File.Delete(path);
        }
        catch (Exception ex) when (IsItemError(ex))
        {
            _logger.LogWarning("Could not remove temporary file {Path}: {Error}", path, ex.Message);
        }
    }

    private void Log(PlannedAction action, ItemOutcome outcome, string? message, string? finalTarget = null)
    {
        var level = outcome == ItemOutcome.Failed ? LogLevel.Warning : LogLevel.Information;
        _logger.Log(level, "{Action} {Source} -> {Target}: {Result} {Error}",
            action.Kind, action.SourcePath ?? "", finalTarget ?? action.TargetPath, outcome, message ?? "");
    }

    private static OverwriteChoice ToChoice(OverwritePolicy policy) => policy switch
    {
        OverwritePolicy.Skip => OverwriteChoice.Skip,
        OverwritePolicy.Overwrite => OverwriteChoice.Overwrite,
        OverwritePolicy.OverwriteIfNewer => OverwriteChoice.OverwriteIfNewer,
        OverwritePolicy.KeepBoth => OverwriteChoice.KeepBoth,
        _ => OverwriteChoice.Skip,
    };

    private static OverwritePolicy ToPolicy(OverwriteChoice choice) => choice switch
    {
        OverwriteChoice.Overwrite => OverwritePolicy.Overwrite,
        OverwriteChoice.OverwriteIfNewer => OverwritePolicy.OverwriteIfNewer,
        OverwriteChoice.KeepBoth => OverwritePolicy.KeepBoth,
        _ => OverwritePolicy.Skip,
    };

    private static bool IsItemError(Exception ex) =>
        ex is IOException or UnauthorizedAccessException or UnsafePathException or ArgumentException or NotSupportedException
            or System.Security.SecurityException;

    private sealed class State(OverwritePolicy policy)
    {
        public OverwritePolicy Policy { get; set; } = policy;
        public bool NewerOverwriteConfirmed { get; set; }
    }
}
