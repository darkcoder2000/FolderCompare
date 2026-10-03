using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;
using Microsoft.Extensions.Logging;

namespace TimeDiff.App.Services;

/// <summary>Loads and saves <see cref="AppSettings"/> as JSON in %APPDATA%\TimeDiff\settings.json.</summary>
public sealed class SettingsService
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        Converters = { new JsonStringEnumConverter() },
    };

    private readonly ILogger<SettingsService> _logger;

    public SettingsService(ILogger<SettingsService> logger)
    {
        _logger = logger;
        Current = Load();
    }

    public static string SettingsDirectory =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "TimeDiff");

    public static string SettingsPath => Path.Combine(SettingsDirectory, "settings.json");

    public AppSettings Current { get; private set; }

    public void Save()
    {
        try
        {
            Directory.CreateDirectory(SettingsDirectory);
            var temp = SettingsPath + ".tmp";
            File.WriteAllText(temp, JsonSerializer.Serialize(Current, JsonOptions));
            File.Move(temp, SettingsPath, overwrite: true);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Could not save settings");
        }
    }

    public static AppSettings Clone(AppSettings settings) =>
        JsonSerializer.Deserialize<AppSettings>(JsonSerializer.Serialize(settings, JsonOptions), JsonOptions)!;

    public void Replace(AppSettings settings)
    {
        Current = settings;
        Save();
    }

    private AppSettings Load()
    {
        try
        {
            if (File.Exists(SettingsPath))
                return JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(SettingsPath), JsonOptions) ?? new AppSettings();
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Could not read settings, using defaults");
        }
        return new AppSettings();
    }
}
