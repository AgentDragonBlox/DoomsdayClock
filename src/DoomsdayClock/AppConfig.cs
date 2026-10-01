using System.Text.Json;
using System.Text.Json.Serialization;

namespace DoomsdayClock;

/// <summary>
/// How the countdown text should be coloured as the deadline approaches.
/// </summary>
public enum ColorMode
{
    /// <summary>Text is always white, regardless of how close the deadline is.</summary>
    StaticWhite,

    /// <summary>Text fades from white to red as the deadline approaches (see
    /// <see cref="ColorInterpolator"/> for the exact curve).</summary>
    GradientToRed,
}

/// <summary>
/// The full set of user-configurable settings, persisted as a small JSON
/// file. This is the single source of truth both the settings UI
/// (<see cref="UI.SettingsForm"/>) and the silent daily updater
/// (<c>Program.cs</c>'s <c>--update</c> path) read from.
/// </summary>
public sealed class AppConfig
{
    /// <summary>The date the countdown is counting down to (time-of-day is ignored).</summary>
    public DateTime TargetDate { get; set; } = DateTime.Today.AddDays(30);

    /// <summary>
    /// The date this countdown was first configured for the *current*
    /// <see cref="TargetDate"/>. This is the "day zero" reference point the
    /// gradient colour mode measures progress from - see
    /// <see cref="ColorInterpolator"/>. It resets automatically whenever the
    /// user picks a new target date, so the gradient always spans the full
    /// life of the countdown currently on screen.
    /// </summary>
    public DateTime CreatedDate { get; set; } = DateTime.Today;

    /// <summary>Whether the wallpaper text stays white or fades to red.</summary>
    public ColorMode ColorMode { get; set; } = ColorMode.StaticWhite;

    /// <summary>
    /// Preferred font family name, e.g. "Gill Sans Nova Ultra Bold". If the
    /// font isn't installed on this machine, <see cref="FontResolver"/> falls
    /// back to the closest available match automatically.
    /// </summary>
    public string FontFamilyName { get; set; } = "Gill Sans Nova Ultra Bold";

    // -- JSON source-generation context ----------------------------------
    //
    // We use System.Text.Json's source generator (AppConfigJsonContext,
    // below) instead of the default reflection-based serializer. For a
    // config this small the difference is measured in microseconds, but it
    // also means the JSON path works correctly under aggressive
    // trimming/AOT in the future without any extra attributes scattered
    // through the model, and it avoids paying for JIT'ing a
    // reflection-based (de)serializer for a one-shot 4-field object that
    // runs once a day.

    /// <summary>Full path to the config file: <c>%AppData%\DoomsdayClock\config.json</c>.</summary>
    public static string ConfigFilePath { get; } = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "DoomsdayClock",
        "config.json");

    /// <summary>
    /// Loads the saved config, or <c>null</c> if the app has never been
    /// configured yet (first run, or the user deleted the file).
    /// </summary>
    public static AppConfig? Load()
    {
        try
        {
            if (!File.Exists(ConfigFilePath))
            {
                return null;
            }

            string json = File.ReadAllText(ConfigFilePath);
            return JsonSerializer.Deserialize(json, AppConfigJsonContext.Default.AppConfig);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException)
        {
            // A corrupted or unreadable config should never crash the silent
            // daily updater - treat it the same as "not configured yet".
            return null;
        }
    }

    /// <summary>Persists this config to <see cref="ConfigFilePath"/>, creating the folder if needed.</summary>
    public void Save()
    {
        string? directory = Path.GetDirectoryName(ConfigFilePath);
        if (!string.IsNullOrEmpty(directory))
        {
            Directory.CreateDirectory(directory);
        }

        string json = JsonSerializer.Serialize(this, AppConfigJsonContext.Default.AppConfig);
        File.WriteAllText(ConfigFilePath, json);
    }
}

/// <summary>
/// Source-generated JSON type information for <see cref="AppConfig"/>.
/// See the comment on <see cref="AppConfig"/>'s SerializerOptions field for why.
/// </summary>
[JsonSourceGenerationOptions(WriteIndented = true, Converters = [typeof(JsonStringEnumConverter)])]
[JsonSerializable(typeof(AppConfig))]
internal partial class AppConfigJsonContext : JsonSerializerContext
{
}
