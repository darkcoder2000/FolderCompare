using System.Globalization;
using System.IO.Abstractions;
using System.Text;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Logging.Abstractions;

namespace FolderCompare.Core.Operations;

/// <summary>
/// The freedesktop.org Trash specification 1.0 (GNOME, KDE, Xfce, ...), used on Linux:
/// items on the home file system go to <c>$XDG_DATA_HOME/Trash</c>; items on other mounts go to
/// <c>$topdir/.Trash/$uid</c> (only if <c>.Trash</c> is a real, sticky directory) or else <c>$topdir/.Trash-$uid</c>.
/// Each item gets a <c>.trashinfo</c> file with its original path and deletion date, so the desktop can restore it.
/// </summary>
public sealed class FreedesktopTrash : IRecycleBin
{
    private const UnixFileMode PrivateDirectory = UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute;

    private readonly IFileSystem _fs;
    private readonly string _homeTrash;
    private readonly string _uid;
    private readonly Func<string, string> _mountPointOf;
    private readonly Func<DateTime> _now;
    private readonly ILogger _logger;

    /// <param name="homeTrash">Usually <see cref="DefaultHomeTrash"/>.</param>
    /// <param name="uid">The numeric user id, used in the per-mount trash folder names.</param>
    /// <param name="mountPointOf">Returns the mount point (top directory) of an absolute path.</param>
    public FreedesktopTrash(IFileSystem fs, string homeTrash, string uid, Func<string, string> mountPointOf,
                            Func<DateTime>? now = null, ILogger? logger = null)
    {
        _fs = fs;
        _homeTrash = fs.Path.TrimEndingDirectorySeparator(homeTrash);
        _uid = uid;
        _mountPointOf = mountPointOf;
        _now = now ?? (() => DateTime.Now);
        _logger = logger ?? NullLogger.Instance;
    }

    /// <summary><c>$XDG_DATA_HOME/Trash</c>, defaulting to <c>~/.local/share/Trash</c>.</summary>
    public static string DefaultHomeTrash
    {
        get
        {
            var dataHome = Environment.GetEnvironmentVariable("XDG_DATA_HOME");
            if (string.IsNullOrEmpty(dataHome) || !Path.IsPathRooted(dataHome))
                dataHome = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".local", "share");
            return Path.Combine(dataHome, "Trash");
        }
    }

    public void DeleteFile(string path) => MoveToTrash(path);
    public void DeleteDirectory(string path) => MoveToTrash(path);

    public bool CanRecycle(string path)
    {
        try
        {
            return FindTrash(_fs.Path.GetFullPath(path), create: false) is not null;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return false;
        }
    }

    private void MoveToTrash(string path)
    {
        var full = _fs.Path.TrimEndingDirectorySeparator(_fs.Path.GetFullPath(path));
        var trash = FindTrash(full, create: true)
                    ?? throw new TrashUnavailableException($"There is no usable trash folder for '{full}'.");

        var files = _fs.Path.Join(trash.Directory, "files");
        var info = _fs.Path.Join(trash.Directory, "info");
        CreatePrivateDirectory(files);
        CreatePrivateDirectory(info);

        var originalPath = trash.TopDir is null ? full : _fs.Path.GetRelativePath(trash.TopDir, full);
        var content = "[Trash Info]\n" +
                      $"Path={Encode(originalPath)}\n" +
                      $"DeletionDate={_now().ToString("yyyy-MM-ddTHH:mm:ss", CultureInfo.InvariantCulture)}\n";

        // Reserve a unique name by creating the .trashinfo file exclusively, then move the item next to it.
        var name = _fs.Path.GetFileName(full);
        for (int n = 1; ; n++)
        {
            var candidate = n == 1 ? name : $"{name}.{n}";
            var infoFile = _fs.Path.Join(info, candidate + ".trashinfo");
            var target = _fs.Path.Join(files, candidate);
            if (_fs.File.Exists(target) || _fs.Directory.Exists(target)) continue;
            try
            {
                using (var stream = _fs.FileStream.New(infoFile, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                {
                    var bytes = Encoding.UTF8.GetBytes(content);
                    stream.Write(bytes, 0, bytes.Length);
                }
            }
            catch (IOException) when (_fs.File.Exists(infoFile))
            {
                continue; // taken by someone else meanwhile
            }

            try
            {
                if (_fs.Directory.Exists(full)) _fs.Directory.Move(full, target);
                else _fs.File.Move(full, target);
            }
            catch
            {
                TryDelete(infoFile);
                throw;
            }
            _logger.LogInformation("Moved {Path} to the trash as {Target}", full, target);
            return;
        }
    }

    private sealed record TrashLocation(string Directory, string? TopDir);

    /// <summary>The trash for <paramref name="fullPath"/>; TopDir is null for the home trash (absolute info paths).</summary>
    private TrashLocation? FindTrash(string fullPath, bool create)
    {
        var mount = _mountPointOf(fullPath);
        if (string.Equals(mount, _mountPointOf(ExistingAncestor(_homeTrash)), StringComparison.Ordinal))
            return new TrashLocation(_homeTrash, null);

        // $topdir/.Trash/$uid when the admin prepared a shared, sticky .Trash folder.
        var shared = _fs.Path.Join(mount, ".Trash");
        if (IsStickyRealDirectory(shared))
        {
            var perUser = _fs.Path.Join(shared, _uid);
            if (_fs.Directory.Exists(perUser)) return new TrashLocation(perUser, mount);
            if (create && TryCreatePrivateDirectory(perUser)) return new TrashLocation(perUser, mount);
        }

        var own = _fs.Path.Join(mount, ".Trash-" + _uid);
        if (_fs.Directory.Exists(own)) return IsLink(own) ? null : new TrashLocation(own, mount);
        if (!create) return _fs.Directory.Exists(mount) ? new TrashLocation(own, mount) : null;
        return TryCreatePrivateDirectory(own) ? new TrashLocation(own, mount) : null;
    }

    private string ExistingAncestor(string path)
    {
        for (var p = path; !string.IsNullOrEmpty(p); p = _fs.Path.GetDirectoryName(p))
            if (_fs.Directory.Exists(p)) return p;
        return _fs.Path.GetPathRoot(path) ?? path;
    }

    private bool IsStickyRealDirectory(string path)
    {
        if (!_fs.Directory.Exists(path) || IsLink(path)) return false;
        try
        {
            return !OperatingSystem.IsWindows() && (_fs.File.GetUnixFileMode(path) & UnixFileMode.StickyBit) != 0;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or NotSupportedException)
        {
            return false;
        }
    }

    private bool IsLink(string path)
    {
        try
        {
            return _fs.DirectoryInfo.New(path).LinkTarget is not null;
        }
        catch (IOException)
        {
            return false;
        }
    }

    private bool TryCreatePrivateDirectory(string path)
    {
        try
        {
            CreatePrivateDirectory(path);
            return true;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            _logger.LogWarning("Could not create the trash folder {Path}: {Error}", path, ex.Message);
            return false;
        }
    }

    private void CreatePrivateDirectory(string path)
    {
        if (_fs.Directory.Exists(path)) return;
        _fs.Directory.CreateDirectory(path);
        if (OperatingSystem.IsWindows()) return;
        try
        {
            _fs.File.SetUnixFileMode(path, PrivateDirectory);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or NotSupportedException)
        {
            // The mode is a recommendation of the spec; the trash works without it.
        }
    }

    private void TryDelete(string file)
    {
        try
        {
            _fs.File.Delete(file);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            _logger.LogWarning("Could not remove {File}: {Error}", file, ex.Message);
        }
    }

    /// <summary>Percent-encodes a path like a file URL (UTF-8; unreserved characters and '/' stay).</summary>
    internal static string Encode(string path)
    {
        var sb = new StringBuilder();
        foreach (var b in Encoding.UTF8.GetBytes(path))
        {
            char c = (char)b;
            if (c is (>= 'A' and <= 'Z') or (>= 'a' and <= 'z') or (>= '0' and <= '9') or '-' or '_' or '.' or '~' or '/')
                sb.Append(c);
            else
                sb.Append('%').Append(b.ToString("X2", CultureInfo.InvariantCulture));
        }
        return sb.ToString();
    }

    /// <summary>
    /// The mount point containing <paramref name="path"/>, from the "mountinfo" table of the kernel
    /// (<c>/proc/self/mountinfo</c>): the longest mount point that is a prefix of the path.
    /// </summary>
    public static string MountPointOf(string path, IEnumerable<string> mountInfoLines)
    {
        string best = "/";
        foreach (var line in mountInfoLines)
        {
            var fields = line.Split(' ');
            if (fields.Length < 5) continue;
            var mount = Unescape(fields[4]);
            bool inside = path == mount || mount == "/" || path.StartsWith(mount.TrimEnd('/') + "/", StringComparison.Ordinal);
            if (inside && mount.Length > best.Length) best = mount;
        }
        return best;
    }

    /// <summary>mountinfo escapes space, tab, newline and backslash as octal (\040 ...).</summary>
    private static string Unescape(string field)
    {
        if (!field.Contains('\\')) return field;
        var sb = new StringBuilder();
        for (int i = 0; i < field.Length; i++)
        {
            if (field[i] == '\\' && i + 3 < field.Length && field.AsSpan(i + 1, 3).IndexOfAnyExceptInRange('0', '7') < 0)
            {
                sb.Append((char)Convert.ToInt32(field.Substring(i + 1, 3), 8));
                i += 3;
            }
            else
            {
                sb.Append(field[i]);
            }
        }
        return sb.ToString();
    }
}
