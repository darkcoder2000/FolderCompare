using System.IO;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Security.Cryptography;
using Microsoft.Extensions.Logging;
using Microsoft.Win32;
using FolderCompare.Core.Shell;

namespace FolderCompare.App.Services;

public interface IExplorerIntegrationService
{
    /// <summary>False on Linux and macOS: there is no Explorer context menu to integrate with.</summary>
    bool IsSupported { get; }
    bool IsRegistered { get; }
    void Register();
    void Unregister();

    /// <summary>Re-registers when the entries point to another exe or DLL, or still use the 1.2.0 layout.</summary>
    void EnsureCurrentPath();
}

/// <summary>
/// Adds the "Compare" / "Select as left" entries to the Explorer context menu (per user, HKCU). The entries are
/// handled by FolderCompare.ShellExt.dll, which is embedded in the exe and extracted to %LOCALAPPDATA%.
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class ExplorerIntegrationService : IExplorerIntegrationService
{
    private const int SHCNE_ASSOCCHANGED = 0x08000000;
    private const uint SHCNF_IDLIST = 0;

    private static readonly string DllDirectory = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "FolderCompare", "ShellExt");

    private readonly ILogger<ExplorerIntegrationService> _logger;
    private byte[]? _dll;

    public ExplorerIntegrationService(ILogger<ExplorerIntegrationService> logger) => _logger = logger;

    public bool IsSupported => true;

    private static string ExePath => Environment.ProcessPath ?? throw new InvalidOperationException("Unknown executable path.");

    private static string ProbeKey =>
        $@"{ExplorerMenuLayout.ClassesRoot}\{ExplorerMenuLayout.VerbKey("Directory", ExplorerMenuLayout.SelectLeftVerb)}";

    public bool IsRegistered
    {
        get
        {
            using var key = Registry.CurrentUser.OpenSubKey(ProbeKey);
            return key is not null;
        }
    }

    private byte[] DllBytes
    {
        get
        {
            if (_dll is not null) return _dll;
            using var stream = typeof(ExplorerIntegrationService).Assembly.GetManifestResourceStream(ExplorerMenuLayout.DllResourceName)
                ?? throw new InvalidOperationException("The shell extension is missing from this build.");
            using var buffer = new MemoryStream();
            stream.CopyTo(buffer);
            return _dll = buffer.ToArray();
        }
    }

    private string ExpectedDllPath =>
        Path.Combine(DllDirectory, ExplorerMenuLayout.DllFileName(Convert.ToHexString(SHA256.HashData(DllBytes))[..8].ToLowerInvariant()));

    public void Register()
    {
        if (RuntimeInformation.OSArchitecture != Architecture.X64)
            throw new PlatformNotSupportedException("The Explorer context menu integration is only available on 64-bit Intel/AMD Windows.");

        var dllPath = ExpectedDllPath;
        if (!File.Exists(dllPath))
        {
            Directory.CreateDirectory(DllDirectory);
            var temp = dllPath + ".tmpcopy";
            File.WriteAllBytes(temp, DllBytes);
            File.Move(temp, dllPath, overwrite: true);
            _logger.LogInformation("Extracted the shell extension to {Dll}", dllPath);
        }

        DeleteVerbKeys();
        foreach (var v in ExplorerMenuLayout.Build(ExePath, dllPath))
        {
            using var key = Registry.CurrentUser.CreateSubKey(v.SubKey);
            key.SetValue(v.Name, v.Value);
        }
        NotifyShell();
        _logger.LogInformation("Registered Explorer context menu for {Exe} with {Dll}", ExePath, dllPath);

        DeleteOldDlls(keep: dllPath);
        DeleteLegacySelectionFile();
    }

    public void Unregister()
    {
        DeleteVerbKeys();
        foreach (var clsid in ExplorerMenuLayout.Clsids)
            Registry.CurrentUser.DeleteSubKeyTree($@"{ExplorerMenuLayout.ClassesRoot}\{ExplorerMenuLayout.ClsidKey(clsid)}", throwOnMissingSubKey: false);
        Registry.CurrentUser.DeleteSubKeyTree(ExplorerMenuLayout.LeftSelectionKey, throwOnMissingSubKey: false);
        using (var app = Registry.CurrentUser.OpenSubKey(ExplorerMenuLayout.AppKey, writable: true))
            app?.DeleteValue(ExplorerMenuLayout.ExePathValue, throwOnMissingValue: false);
        NotifyShell();
        _logger.LogInformation("Removed Explorer context menu");
        DeleteOldDlls(keep: null);
    }

    public void EnsureCurrentPath()
    {
        try
        {
            if (!IsRegistered) return;
            var legacy = ExplorerMenuLayout.LegacyKeys().FirstOrDefault(KeyExists);
            if (legacy is not null)
            {
                _logger.LogInformation("Legacy Explorer context menu entries found ({Key}), updating", legacy);
                Register();
                return;
            }
            string? exe, dll;
            using (var app = Registry.CurrentUser.OpenSubKey(ExplorerMenuLayout.AppKey))
                exe = app?.GetValue(ExplorerMenuLayout.ExePathValue) as string;
            using (var server = Registry.CurrentUser.OpenSubKey(ExplorerMenuLayout.InprocServerKey(ExplorerMenuLayout.CompareClsid)))
                dll = server?.GetValue("") as string;
            if (string.Equals(exe, ExePath, StringComparison.OrdinalIgnoreCase)
                && string.Equals(dll, ExpectedDllPath, StringComparison.OrdinalIgnoreCase)
                && File.Exists(dll))
                return;
            _logger.LogInformation("Explorer context menu points to {OldExe} / {OldDll}, updating", exe, dll);
            Register();
        }
        catch (Exception ex)
        {
            _logger.LogWarning(ex, "Could not check the Explorer context menu registration");
        }
    }

    private static bool KeyExists(string subKey)
    {
        using var key = Registry.CurrentUser.OpenSubKey(subKey);
        return key is not null;
    }

    private static void DeleteVerbKeys()
    {
        foreach (var key in ExplorerMenuLayout.VerbKeys())
            Registry.CurrentUser.DeleteSubKeyTree($@"{ExplorerMenuLayout.ClassesRoot}\{key}", throwOnMissingSubKey: false);
    }

    /// <summary>Removes DLLs of earlier versions. A DLL that Explorer still has loaded stays until the next try.</summary>
    private void DeleteOldDlls(string? keep)
    {
        if (!Directory.Exists(DllDirectory)) return;
        foreach (var file in Directory.EnumerateFiles(DllDirectory, "FolderCompare.ShellExt.*"))
        {
            if (keep is not null && string.Equals(file, keep, StringComparison.OrdinalIgnoreCase)) continue;
            try
            {
                File.Delete(file);
                _logger.LogInformation("Deleted old shell extension {Dll}", file);
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                _logger.LogInformation("Old shell extension {Dll} is still in use, keeping it", file);
            }
        }
    }

    /// <summary>Version 1.2.0 kept the left item in a file; the DLL now keeps it in the registry.</summary>
    private void DeleteLegacySelectionFile()
    {
        try
        {
            var file = Path.Combine(SettingsService.SettingsDirectory, "explorer-left.txt");
            if (!File.Exists(file)) return;
            File.Delete(file);
            _logger.LogInformation("Deleted legacy selection file {File}", file);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            _logger.LogWarning(ex, "Could not delete the legacy selection file");
        }
    }

    private static void NotifyShell() => SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, IntPtr.Zero, IntPtr.Zero);

    [DllImport("shell32.dll")]
    private static extern void SHChangeNotify(int wEventId, uint uFlags, IntPtr dwItem1, IntPtr dwItem2);
}

/// <summary>Used on Linux and macOS.</summary>
public sealed class NoExplorerIntegration : IExplorerIntegrationService
{
    public bool IsSupported => false;
    public bool IsRegistered => false;
    public void Register() => throw new PlatformNotSupportedException("The Explorer context menu exists only on Windows.");
    public void Unregister() { }
    public void EnsureCurrentPath() { }
}
