using System.Diagnostics;
using System.Security.Principal;
using System.Text;

namespace DoomsdayClock;

/// <summary>
/// Wraps the built-in <c>schtasks.exe</c> command-line tool to install,
/// query and remove the per-user Scheduled Task that runs the app's
/// silent <c>--update</c> path once a day.
///
/// Why shell out to schtasks.exe instead of taking a Task Scheduler COM /
/// NuGet library dependency? Three reasons:
///   1. schtasks.exe ships with every copy of Windows - zero extra
///      dependency, zero extra published bytes.
///   2. The whole interaction happens once, when the user clicks
///      "Save &amp; Activate" - process-launch overhead (a few ms) is
///      completely irrelevant at that frequency.
///   3. It sidesteps the COM interop lifetime/threading footguns of
///      TaskScheduler.dll (STA requirements, RCW cleanup) for a one-shot
///      operation that doesn't need them.
/// </summary>
public static class TaskSchedulerService
{
    /// <summary>
    /// Task Scheduler folder + name. Namespaced under a folder so it's easy
    /// for the user to find (and delete, if they ever uninstall by hand) in
    /// the Task Scheduler UI, and so it can never collide with an unrelated
    /// task.
    /// </summary>
    private const string TaskName = @"DoomsdayClock\DailyWallpaperUpdate";

    /// <summary>Local time-of-day the daily update runs, in 24h "HH:mm" format.</summary>
    private const string DailyRunTime = "00:05";

    /// <summary>
    /// Creates (or replaces) the scheduled task so it launches
    /// <paramref name="exePath"/> with the <c>--update</c> argument:
    ///   * every day at <see cref="DailyRunTime"/>, and
    ///   * again at logon (so a machine that was asleep/off at 00:05 still
    ///     catches up as soon as the user signs back in).
    /// Runs at standard user privilege - no UAC prompt.
    /// </summary>
    public static bool CreateOrUpdateTask(string exePath)
    {
        string xmlPath = Path.Combine(Path.GetTempPath(), $"DoomsdayClock_Task_{Guid.NewGuid():N}.xml");
        try
        {
            File.WriteAllText(xmlPath, BuildTaskXml(exePath), Encoding.Unicode);
            return RunSchtasks($"/Create /TN \"{TaskName}\" /XML \"{xmlPath}\" /F");
        }
        finally
        {
            TryDelete(xmlPath);
        }
    }

    /// <summary>True if the scheduled task currently exists.</summary>
    public static bool IsTaskInstalled()
    {
        return RunSchtasks($"/Query /TN \"{TaskName}\"");
    }

    /// <summary>Removes the scheduled task, if present. Safe to call when it doesn't exist.</summary>
    public static bool RemoveTask()
    {
        return RunSchtasks($"/Delete /TN \"{TaskName}\" /F");
    }

    private static void TryDelete(string path)
    {
        try
        {
            File.Delete(path);
        }
        catch (IOException)
        {
            // Best-effort cleanup of a temp file; leaving one stray file
            // behind is not worth surfacing an error for.
        }
    }

    private static bool RunSchtasks(string arguments)
    {
        // Deliberately not redirecting stdout/stderr: we only ever care
        // about the exit code, and redirecting without draining the pipes
        // risks a classic deadlock if schtasks ever writes more than the OS
        // pipe buffer holds. CreateNoWindow keeps the console invisible
        // either way.
        var startInfo = new ProcessStartInfo("schtasks.exe", arguments)
        {
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        using Process? process = Process.Start(startInfo);
        if (process is null)
        {
            return false;
        }

        process.WaitForExit();
        return process.ExitCode == 0;
    }

    /// <summary>
    /// Builds the Task Scheduler XML definition. XML (rather than chained
    /// <c>/SC DAILY</c> command-line flags) is what lets a single task
    /// carry two independent triggers - daily-at-a-time *and* at-logon -
    /// which the flag-based schtasks syntax cannot express in one call.
    /// </summary>
    private static string BuildTaskXml(string exePath)
    {
        string sid = WindowsIdentity.GetCurrent().User?.Value
            ?? throw new InvalidOperationException("Could not resolve the current user's SID.");

        DateTime firstRun = DateTime.Today.AddDays(1).Add(TimeSpan.Parse(DailyRunTime));
        string startBoundary = firstRun.ToString("yyyy-MM-ddTHH:mm:ss");

        // XML-escape the exe path defensively (install paths containing
        // '&' are rare but not impossible).
        string escapedExePath = System.Security.SecurityElement.Escape(exePath) ?? exePath;

        return $"""
        <?xml version="1.0" encoding="UTF-16"?>
        <Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
          <RegistrationInfo>
            <Description>Updates the Doomsday Clock desktop wallpaper once a day.</Description>
          </RegistrationInfo>
          <Triggers>
            <CalendarTrigger>
              <StartBoundary>{startBoundary}</StartBoundary>
              <Enabled>true</Enabled>
              <ScheduleByDay>
                <DaysInterval>1</DaysInterval>
              </ScheduleByDay>
            </CalendarTrigger>
            <LogonTrigger>
              <Enabled>true</Enabled>
              <UserId>{sid}</UserId>
            </LogonTrigger>
          </Triggers>
          <Principals>
            <Principal id="Author">
              <UserId>{sid}</UserId>
              <LogonType>InteractiveToken</LogonType>
              <RunLevel>LeastPrivilege</RunLevel>
            </Principal>
          </Principals>
          <Settings>
            <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
            <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
            <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
            <AllowHardTerminate>true</AllowHardTerminate>
            <StartWhenAvailable>true</StartWhenAvailable>
            <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
            <Enabled>true</Enabled>
            <Hidden>false</Hidden>
            <ExecutionTimeLimit>PT1M</ExecutionTimeLimit>
            <Priority>7</Priority>
          </Settings>
          <Actions Context="Author">
            <Exec>
              <Command>"{escapedExePath}"</Command>
              <Arguments>--update</Arguments>
            </Exec>
          </Actions>
        </Task>
        """;
    }
}
