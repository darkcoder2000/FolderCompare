using System.IO.Abstractions;
using FolderCompare.Core.Models;

namespace FolderCompare.Core.Comparison;

/// <summary>Raw scan result of one side, before merging.</summary>
internal sealed class ScanEntry
{
    public required string Name { get; init; }
    public required FileInfoSnapshot Snapshot { get; init; }
    public string? Error { get; set; }
    /// <summary>Non-null for folders whose contents were enumerated.</summary>
    public Dictionary<string, ScanEntry>? Children { get; set; }
    public bool IsDirectory => Snapshot.IsDirectory;
}

/// <summary>
/// Streams one directory tree into <see cref="ScanEntry"/> objects. Read-only: never modifies the file system.
/// </summary>
internal sealed class DirectoryScanner
{
    internal const int MaxDepth = 512;
    private const FileAttributes KeptAttributes = FileAttributes.Directory | FileAttributes.ReadOnly | FileAttributes.Hidden |
                                                  FileAttributes.System | FileAttributes.Archive | FileAttributes.ReparsePoint;

    private readonly IFileSystem _fs;

    public DirectoryScanner(IFileSystem fs) => _fs = fs;

    public ScanEntry ScanRoot(string root, CompareOptions options, ProgressTracker? tracker, CancellationToken ct)
    {
        var info = _fs.DirectoryInfo.New(root);
        var entry = new ScanEntry { Name = "", Snapshot = Snapshot(info, root, 0) };
        var ctx = new Context(options, new PathPatternMatcher(options.IncludePatterns, options.ExcludePatterns), tracker, ct);
        var ancestors = new HashSet<string>(StringComparer.OrdinalIgnoreCase) { Key(root) };
        Fill(entry, root, root, "", ctx, ancestors, 0);
        return entry;
    }

    /// <summary>Scans a single relative path (file or folder subtree). Returns null if it does not exist.</summary>
    public ScanEntry? ScanPath(string root, string relativePath, CompareOptions options, CancellationToken ct)
    {
        var full = _fs.Path.Join(root, relativePath);
        var name = _fs.Path.GetFileName(full);
        var ctx = new Context(options, new PathPatternMatcher(options.IncludePatterns, options.ExcludePatterns), null, ct);

        if (_fs.Directory.Exists(full))
        {
            var info = _fs.DirectoryInfo.New(full);
            bool isLink = IsLink(info);
            if (isLink && options.SymlinkMode == SymlinkMode.Skip) return null;
            var entry = new ScanEntry { Name = name, Snapshot = Snapshot(info, full, 0) };
            if (options.Recursive)
            {
                string enumPath = full;
                if (isLink && !TryResolve(info, entry, out enumPath)) return entry;
                var ancestors = new HashSet<string>(StringComparer.OrdinalIgnoreCase) { Key(root), Key(enumPath) };
                Fill(entry, enumPath, full, relativePath, ctx, ancestors, 0);
            }
            return entry;
        }

        if (_fs.File.Exists(full))
        {
            var info = _fs.FileInfo.New(full);
            if (IsLink(info) && options.SymlinkMode == SymlinkMode.Skip) return null;
            return FileEntry(info, name, full, options);
        }

        return null;
    }

    public FileInfoSnapshot? Stat(string fullPath)
    {
        try
        {
            if (_fs.Directory.Exists(fullPath)) return Snapshot(_fs.DirectoryInfo.New(fullPath), fullPath, 0);
            if (_fs.File.Exists(fullPath))
            {
                var fi = _fs.FileInfo.New(fullPath);
                return Snapshot(fi, fullPath, fi.Length);
            }
        }
        catch (Exception ex) when (IsIoError(ex))
        {
        }
        return null;
    }

    private void Fill(ScanEntry entry, string enumPath, string fullPath, string rel, Context ctx,
                      HashSet<string> ancestors, int depth)
    {
        ctx.Ct.ThrowIfCancellationRequested();
        ctx.Tracker?.Folder(fullPath);
        var children = new Dictionary<string, ScanEntry>(StringComparer.OrdinalIgnoreCase);
        entry.Children = children;

        IEnumerator<IFileSystemInfo> e;
        try
        {
            e = _fs.DirectoryInfo.New(enumPath).EnumerateFileSystemInfos().GetEnumerator();
        }
        catch (Exception ex) when (IsIoError(ex))
        {
            MarkError(entry, ex.Message);
            return;
        }

        using (e)
        {
            while (true)
            {
                ctx.Ct.ThrowIfCancellationRequested();
                IFileSystemInfo info;
                try
                {
                    if (!e.MoveNext()) break;
                    info = e.Current;
                }
                catch (Exception ex) when (IsIoError(ex))
                {
                    MarkError(entry, ex.Message);
                    return;
                }

                var child = ScanChild(info, enumPath, fullPath, rel, ctx, ancestors, depth);
                if (child is not null) children[child.Name] = child;
            }
        }
    }

    private ScanEntry? ScanChild(IFileSystemInfo info, string enumPath, string fullPath, string rel, Context ctx,
                                 HashSet<string> ancestors, int depth)
    {
        string name = info.Name;
        string childRel = rel.Length == 0 ? name : rel + "\\" + name;
        string childFull = _fs.Path.Join(fullPath, name);

        FileAttributes attrs;
        try
        {
            attrs = info.Attributes;
        }
        catch (Exception ex) when (IsIoError(ex))
        {
            ctx.Tracker?.Item();
            return new ScanEntry { Name = name, Snapshot = new FileInfoSnapshot(0, default, 0, childFull), Error = ex.Message };
        }

        if (ctx.Options.IgnoreHiddenAndSystem && (attrs & (FileAttributes.Hidden | FileAttributes.System)) != 0) return null;
        bool isDir = (attrs & FileAttributes.Directory) != 0;
        if (ctx.Matcher.IsExcluded(name, childRel)) return null;
        if (!isDir && !ctx.Matcher.IsIncludedFile(name, childRel)) return null;

        bool isLink = (attrs & FileAttributes.ReparsePoint) != 0 && IsLink(info);
        if (isLink && ctx.Options.SymlinkMode == SymlinkMode.Skip) return null;

        ctx.Tracker?.Item();

        if (!isDir)
        {
            try
            {
                return FileEntry(info, name, childFull, ctx.Options);
            }
            catch (Exception ex) when (IsIoError(ex))
            {
                return new ScanEntry { Name = name, Snapshot = new FileInfoSnapshot(0, default, attrs, childFull), Error = ex.Message };
            }
        }

        var entry = new ScanEntry { Name = name, Snapshot = Snapshot(info, childFull, 0) };
        if (!ctx.Options.Recursive) return entry;

        string childEnum = _fs.Path.Join(enumPath, name);
        if (isLink && !TryResolve(info, entry, out childEnum)) return entry;

        var key = Key(childEnum);
        if (depth >= MaxDepth || ancestors.Contains(key))
        {
            MarkError(entry, "Symbolic link loop detected - folder skipped.");
            return entry;
        }

        ancestors.Add(key);
        try
        {
            Fill(entry, childEnum, childFull, childRel, ctx, ancestors, depth + 1);
        }
        finally
        {
            ancestors.Remove(key);
        }
        return entry;
    }

    private static ScanEntry FileEntry(IFileSystemInfo info, string name, string fullPath, CompareOptions options)
    {
        IFileSystemInfo source = info;
        if (options.SymlinkMode == SymlinkMode.Follow && IsLink(info))
            source = info.ResolveLinkTarget(true) ?? info;
        long size = source is IFileInfo fi ? fi.Length : 0;
        var snap = new FileInfoSnapshot(size, source.LastWriteTimeUtc, info.Attributes & KeptAttributes, fullPath);
        return new ScanEntry { Name = name, Snapshot = snap };
    }

    private static bool TryResolve(IFileSystemInfo link, ScanEntry entry, out string target)
    {
        target = "";
        try
        {
            var resolved = link.ResolveLinkTarget(true);
            if (resolved is null || !resolved.Exists)
            {
                MarkError(entry, "Link target does not exist.");
                return false;
            }
            target = resolved.FullName;
            return true;
        }
        catch (Exception ex) when (IsIoError(ex))
        {
            MarkError(entry, ex.Message);
            return false;
        }
    }

    private static bool IsLink(IFileSystemInfo info)
    {
        try
        {
            return info.LinkTarget is not null;
        }
        catch (Exception ex) when (IsIoError(ex))
        {
            return false;
        }
    }

    private static void MarkError(ScanEntry entry, string message)
    {
        entry.Error = message;
        entry.Children = null;
    }

    private static FileInfoSnapshot Snapshot(IFileSystemInfo info, string fullPath, long size) =>
        new(size, info.LastWriteTimeUtc, info.Attributes & KeptAttributes, fullPath);

    private string Key(string path) => _fs.Path.TrimEndingDirectorySeparator(_fs.Path.GetFullPath(path));

    internal static bool IsIoError(Exception ex) =>
        ex is IOException or UnauthorizedAccessException or System.Security.SecurityException or ArgumentException or NotSupportedException;

    private sealed record Context(CompareOptions Options, PathPatternMatcher Matcher, ProgressTracker? Tracker, CancellationToken Ct);
}
