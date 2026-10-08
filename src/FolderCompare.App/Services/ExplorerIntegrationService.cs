using System.Runtime.InteropServices;
using Microsoft.Extensions.Logging;
using Microsoft.Win32;
using FolderCompare.Core.Shell;

namespace FolderCompare.App.Services;

public interface IExplorerIntegrationService
{
    bool IsRegistered { get; }
    void Register();
    void Unregister();

    /// <summary>Rewrites the "Compare to" label for the given pending left item (null = none). No-op when not registered.</summary>
    void UpdateLabel(string? pendingLeft);

    /// <summary>Re-registers when the entries point to another exe (the exe was moved).</summary>
    void EnsureCurrentPath();
}

/// <summary>Adds the "Select as left" / "Compare to left" entries to the Explorer context menu (per user, HKCU).</summary>
public sealed class ExplorerIntegrationService : IExplorerIntegrationService
{
    private const string ClassesRoot = @"Software\Classes";
    private const int SHCNE_ASSOCCHANGED = 0x08000000;
    private const uint SHCNF_IDLIST = 0;

    private readonly ExplorerSelectionStore _store;
    private readonly ILogger<ExplorerIntegrationService> _logger;

    public ExplorerIntegrationService(ExplorerSelectionStore store, ILogger<ExplorerIntegrationService> logger)
    {
        _store = store;
        _logger = logger;
    }

    private static string ExePath => Environment.ProcessPath ?? throw new InvalidOperationException("Unknown executable path.");

    private static string ProbeKey =>
        $@"{ClassesRoot}\{ExplorerMenuLayout.VerbKey("Directory", ExplorerMenuLayout.SelectLeftVerb)}\command";

    public bool IsRegistered
    {
        get
        {
            using var key = Registry.CurrentUser.OpenSubKey(ProbeKey);
            return key is not null;
        }
    }

    public void Register()
    {
        Write(ExplorerMenuLayout.Build(ExePath, _store.Read()));
        _logger.LogInformation("Registered Explorer context menu for {Exe}", ExePath);
    }

    public void Unregister()
    {
        foreach (var key in ExplorerMenuLayout.VerbKeys())
            Registry.CurrentUser.DeleteSubKeyTree($@"{ClassesRoot}\{key}", throwOnMissingSubKey: false);
        _store.Clear();
        NotifyShell();
        _logger.LogInformation("Removed Explorer context menu");
    }

    public void UpdateLabel(string? pendingLeft)
    {
        if (!IsRegistered) return;
        var label = ExplorerMenuLayout.CompareLabel(pendingLeft);
        foreach (var classKey in ExplorerMenuLayout.ClassKeys)
        {
            using var key = Registry.CurrentUser.CreateSubKey(
                $@"{ClassesRoot}\{ExplorerMenuLayout.VerbKey(classKey, ExplorerMenuLayout.CompareToLeftVerb)}");
            key.SetValue("MUIVerb", label);
        }
    }

    public void EnsureCurrentPath()
    {
        try
        {
            string? current;
            using (var key = Registry.CurrentUser.OpenSubKey(ProbeKey))
            {
                if (key is null) return;
                current = key.GetValue("") as string;
            }
            var expected = ExplorerMenuLayout.Command(ExePath, ExplorerMenuLayout.SelectLeftSwitch);
            if (string.Equals(current, expected, StringComparison.OrdinalIgnoreCase)) return;
            _logger.LogInformation("Explorer context menu points to {Old}, updating", current);
            Register();
        }
        catch (Exception ex)
        {
            _logger.LogWarning(ex, "Could not check the Explorer context menu registration");
        }
    }

    private static void Write(IEnumerable<RegistryValue> values)
    {
        foreach (var v in values)
        {
            using var key = Registry.CurrentUser.CreateSubKey($@"{ClassesRoot}\{v.SubKey}");
            key.SetValue(v.Name, v.Value);
        }
        NotifyShell();
    }

    private static void NotifyShell() => SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, IntPtr.Zero, IntPtr.Zero);

    [DllImport("shell32.dll")]
    private static extern void SHChangeNotify(int wEventId, uint uFlags, IntPtr dwItem1, IntPtr dwItem2);
}
