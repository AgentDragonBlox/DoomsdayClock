using System.Runtime.InteropServices;

namespace DoomsdayClock.Native;

/// <summary>
/// All raw Win32 P/Invoke signatures live in this one file, isolated from
/// application logic, so the "unsafe interop surface" of the app is easy to
/// audit at a glance - see README.md "Native interop" section.
/// </summary>
internal static partial class NativeMethods
{
    // -- SystemParametersInfo: used to actually set the desktop wallpaper --

    internal const uint SPI_SETDESKWALLPAPER = 0x0014;
    internal const uint SPIF_UPDATEINIFILE = 0x01;
    internal const uint SPIF_SENDCHANGE = 0x02;

    [LibraryImport("user32.dll", EntryPoint = "SystemParametersInfoW", StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static partial bool SystemParametersInfo(uint uiAction, uint uiParam, string pvParam, uint fWinIni);

    // -- Custom title-bar dragging for the borderless settings window --

    internal const int WM_NCLBUTTONDOWN = 0x00A1;
    internal const int HTCAPTION = 0x0002;

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static partial bool ReleaseCapture();

    [LibraryImport("user32.dll", EntryPoint = "SendMessageW")]
    internal static partial IntPtr SendMessage(IntPtr hWnd, int msg, IntPtr wParam, IntPtr lParam);
}
