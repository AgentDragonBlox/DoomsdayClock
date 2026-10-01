using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using DoomsdayClock.Native;
using Microsoft.Win32;

namespace DoomsdayClock;

/// <summary>
/// Renders today's wallpaper and hands it to Windows. This is the only
/// class that touches the filesystem cache location and the wallpaper
/// registry keys - everything else works purely in terms of
/// <see cref="AppConfig"/> and <see cref="Bitmap"/>.
/// </summary>
public static class WallpaperSetter
{
    /// <summary>
    /// The one file Windows is told to use as the wallpaper. We always
    /// overwrite the same path (rather than writing a new dated file every
    /// day) so we never leak disk space and Explorer's wallpaper history
    /// doesn't fill up with hundreds of near-identical images over a year.
    /// </summary>
    public static string WallpaperFilePath { get; } = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "DoomsdayClock",
        "wallpaper.bmp");

    /// <summary>
    /// Renders <paramref name="config"/> for <paramref name="today"/>,
    /// writes it to <see cref="WallpaperFilePath"/>, and applies it as the
    /// desktop wallpaper across the full virtual desktop (all monitors).
    /// </summary>
    public static void ApplyForToday(AppConfig config, DateTime today)
    {
        // SystemInformation.VirtualScreen spans every monitor's combined
        // bounds, not just the primary display, so multi-monitor users get
        // one continuous countdown rather than it only covering monitor 1.
        Size canvasSize = SystemInformation.VirtualScreen.Size;

        using (Bitmap wallpaper = WallpaperRenderer.Render(config, today, canvasSize))
        {
            string? directory = Path.GetDirectoryName(WallpaperFilePath);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }

            // BMP is the one format guaranteed to work with the classic
            // SPI_SETDESKWALLPAPER call on every Windows version back to
            // XP. PNG/JPG do work on modern Windows too, but BMP carries no
            // compatibility risk at all, and the file lives entirely inside
            // our own AppData folder so its larger size is irrelevant.
            wallpaper.Save(WallpaperFilePath, ImageFormat.Bmp);
        }

        SetWallpaperStyle(spanAcrossMonitors: true);

        bool ok = NativeMethods.SystemParametersInfo(
            NativeMethods.SPI_SETDESKWALLPAPER,
            0,
            WallpaperFilePath,
            NativeMethods.SPIF_UPDATEINIFILE | NativeMethods.SPIF_SENDCHANGE);

        if (!ok)
        {
            throw new InvalidOperationException(
                $"Windows rejected the wallpaper change (Win32 error {Marshal.GetLastWin32Error()}).");
        }
    }

    /// <summary>
    /// Sets the "Span" wallpaper style via the per-user registry keys
    /// Explorer reads, so a single wide image stretches correctly across
    /// every monitor instead of being tiled or centred on just one.
    /// </summary>
    private static void SetWallpaperStyle(bool spanAcrossMonitors)
    {
        using RegistryKey? key = Registry.CurrentUser.OpenSubKey(@"Control Panel\Desktop", writable: true);
        if (key is null)
        {
            return;
        }

        // WallpaperStyle: "22" = Span (Windows 8+), "10" = Fill.
        key.SetValue("WallpaperStyle", spanAcrossMonitors ? "22" : "10");
        key.SetValue("TileWallpaper", "0");
    }
}
