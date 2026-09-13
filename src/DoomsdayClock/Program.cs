using System.Windows.Forms;
using DoomsdayClock.UI;

namespace DoomsdayClock;

/// <summary>
/// Entry point. This one executable serves two completely different
/// purposes depending on how it's launched, rather than shipping as two
/// separate binaries:
///
///   * No arguments -&gt; opens the settings window (the user double-clicked
///     it, or launched it from the Start Menu).
///   * <c>--update</c>       -&gt; silently regenerates and applies today's
///     wallpaper, then exits immediately. No window is ever created on this
///     path. This is what the Scheduled Task
///     (<see cref="TaskSchedulerService"/>) invokes once a day.
///   * <c>--uninstall</c>    -&gt; removes the Scheduled Task and deletes the
///     saved config, then exits. Exposed as a command-line switch (in
///     addition to the "Disable Updates" button in the UI) so the app can
///     be cleanly removed from a script or an uninstaller shortcut.
///
/// Keeping both entry paths in one small, dependency-free executable means
/// the Scheduled Task never has to know about a second file on disk, and a
/// portable single-file deployment stays a single file.
/// </summary>
internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        string? mode = args.Length > 0 ? args[0].Trim().ToLowerInvariant() : null;

        return mode switch
        {
            "--update" => RunSilentUpdate(),
            "--uninstall" => RunUninstall(),
            _ => RunSettingsUi(),
        };
    }

    private static int RunSettingsUi()
    {
        ApplicationConfiguration.Initialize();
        Application.Run(new SettingsForm());
        return 0;
    }

    /// <summary>
    /// The daily background path. Deliberately silent: if anything goes
    /// wrong here (no config yet, wallpaper API rejects the call, etc.) we
    /// exit quietly rather than popping a message box with nobody watching
    /// a scheduled background task at 12:05 AM.
    /// </summary>
    private static int RunSilentUpdate()
    {
        AppConfig? config = AppConfig.Load();
        if (config is null)
        {
            // Never configured (or the config was deleted) - nothing to do.
            return 0;
        }

        try
        {
            WallpaperSetter.ApplyForToday(config, DateTime.Today);
            return 0;
        }
        catch (Exception)
        {
            // Swallow: a background task has no one to report to, and a
            // missed wallpaper refresh is not worth crashing over. The next
            // scheduled run (tomorrow, or at next logon) will simply try again.
            return 1;
        }
    }

    private static int RunUninstall()
    {
        TaskSchedulerService.RemoveTask();

        try
        {
            if (File.Exists(AppConfig.ConfigFilePath))
            {
                File.Delete(AppConfig.ConfigFilePath);
            }
        }
        catch (IOException)
        {
            // Not fatal - the scheduled task (the part that actually runs
            // unattended) is already gone.
        }

        return 0;
    }
}
