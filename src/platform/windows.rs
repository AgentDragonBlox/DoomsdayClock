//! The Windows implementation: wallpaper, scheduling, window effects, file
//! dialogs. All `unsafe` FFI in the project lives in this one file.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmGetColorizationColor, DwmSetWindowAttribute,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    DWPOS_FILL, DesktopWallpaper, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FileOpenDialog, IDesktopWallpaper,
    IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, SPI_SETDESKWALLPAPER, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
    SystemParametersInfoW,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

use super::{Monitor, UiFont};
use crate::config::{Rotation, Skin};

/// Keeps child processes (schtasks.exe, a console app) from flashing a
/// console window, since we are a GUI-subsystem exe.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const TASK_NAME: &str = r"DoomsdayClock\WallpaperUpdate";
/// v1 (C#) registered under this name; removed on install so an old exe
/// isn't also run every day.
const LEGACY_TASK_NAME: &str = r"DoomsdayClock\DailyWallpaperUpdate";

/// COM must be initialised once per thread before any COM call. Calling it
/// again is harmless (S_FALSE), and RPC_E_CHANGED_MODE just means the UI
/// toolkit got there first in a compatible apartment - fine either way.
fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// Called before the headless update: physical-pixel monitor sizes (no DPI
/// virtualisation) and COM.
pub fn prepare_headless() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    com_init();
}

fn desktop_wallpaper() -> windows::core::Result<IDesktopWallpaper> {
    com_init();
    unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL) }
}

/// Every *attached* monitor with its real resolution, via IDesktopWallpaper
/// (Windows 8+). Falls back to the primary screen if that's unavailable.
///
/// Rendering one image per monitor (instead of one image spanned across all
/// of them) is what keeps the text centred on each screen rather than
/// split across the bezel between two monitors.
pub fn monitors() -> Vec<Monitor> {
    let listed = desktop_wallpaper().ok().map(|dw| unsafe {
        let count = dw.GetMonitorDevicePathCount().unwrap_or(0);
        let mut out = Vec::new();
        for i in 0..count {
            let Ok(path) = dw.GetMonitorDevicePathAt(i) else { continue };
            let id = path.to_string().unwrap_or_default();
            let rect: windows::core::Result<RECT> = dw.GetMonitorRECT(PCWSTR(path.0));
            CoTaskMemFree(Some(path.0 as *const c_void));
            // Disconnected monitors are still listed but have no rectangle.
            let Ok(r) = rect else { continue };
            let (width, height) = ((r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32);
            if width > 0 && height > 0 && !id.is_empty() {
                out.push(Monitor { id, width, height });
            }
        }
        out
    });
    match listed {
        Some(list) if !list.is_empty() => list,
        _ => {
            let (width, height) = primary_screen_size();
            vec![Monitor { id: String::new(), width, height }]
        }
    }
}

pub fn primary_screen_size() -> (u32, u32) {
    unsafe {
        let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        if w > 0 && h > 0 { (w as u32, h as u32) } else { (1920, 1080) }
    }
}

pub fn apply_wallpapers(targets: &[(Monitor, PathBuf)]) -> Result<(), String> {
    if targets.iter().all(|(m, _)| !m.id.is_empty())
        && let Ok(dw) = desktop_wallpaper()
    {
        let per_monitor_ok = unsafe {
            let _ = dw.SetPosition(DWPOS_FILL);
            targets
                .iter()
                .all(|(m, path)| dw.SetWallpaper(&HSTRING::from(m.id.as_str()), &HSTRING::from(path.as_path())).is_ok())
        };
        if per_monitor_ok {
            return Ok(());
        }
    }

    // Classic single-image API: works on every Windows version.
    let (_, path) = targets.first().ok_or("No monitors were found.")?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(wide.as_ptr() as *mut c_void),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    }
    .map_err(|e| format!("Windows rejected the wallpaper change: {e}"))
}

// ---------------------------------------------------------------------------
// Task Scheduler
// ---------------------------------------------------------------------------

fn schtasks(args: &[&str]) -> bool {
    Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Registers (or replaces) the per-user task that runs `--update`:
/// * daily at 00:05 - the countdown ticks over,
/// * at every logon - catches up if the PC was off at midnight,
/// * every N hours - only for a custom quote interval.
///
/// Runs with standard user rights: no UAC prompt, no admin needed.
pub fn install_task(exe: &Path, rotation: Rotation) -> Result<(), String> {
    let xml = task_xml(exe, rotation);
    let file = std::env::temp_dir().join(format!("DoomsdayClock-task-{}.xml", std::process::id()));
    // schtasks wants UTF-16 with a BOM for XML definitions.
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let ok = schtasks(&["/Create", "/TN", TASK_NAME, "/XML", &file.to_string_lossy(), "/F"]);
    let _ = std::fs::remove_file(&file);
    schtasks(&["/Delete", "/TN", LEGACY_TASK_NAME, "/F"]);
    if ok { Ok(()) } else { Err("Windows wouldn't create the scheduled task.".into()) }
}

pub fn remove_task() -> Result<(), String> {
    schtasks(&["/Delete", "/TN", LEGACY_TASK_NAME, "/F"]);
    if schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]) || !task_installed() {
        Ok(())
    } else {
        Err("Windows wouldn't remove the scheduled task.".into())
    }
}

pub fn task_installed() -> bool {
    schtasks(&["/Query", "/TN", TASK_NAME])
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn task_xml(exe: &Path, rotation: Rotation) -> String {
    let now = chrono::Local::now().naive_local();
    let fmt = "%Y-%m-%dT%H:%M:%S";
    let tomorrow = (now.date() + chrono::Duration::days(1)).and_hms_opt(0, 5, 0).unwrap_or(now);
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    let user = xml_escape(&user);
    let exe = xml_escape(&exe.to_string_lossy());
    let repeat = match rotation.sanitized() {
        Rotation::EveryHours(h) => format!(
            "<TimeTrigger><Repetition><Interval>PT{h}H</Interval><StopAtDurationEnd>false</StopAtDurationEnd></Repetition>\
             <StartBoundary>{}</StartBoundary><Enabled>true</Enabled></TimeTrigger>",
            (now + chrono::Duration::hours(h as i64)).format(fmt)
        ),
        _ => String::new(),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Refreshes the Doomsday Clock countdown wallpaper and rotates its quote.</Description></RegistrationInfo>
  <Triggers>
    <CalendarTrigger><StartBoundary>{start}</StartBoundary><Enabled>true</Enabled><ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay></CalendarTrigger>
    <LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger>
    {repeat}
  </Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <ExecutionTimeLimit>PT2M</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>"{exe}"</Command><Arguments>--update</Arguments></Exec></Actions>
</Task>"#,
        start = tomorrow.format(fmt),
    )
}

// ---------------------------------------------------------------------------
// Shell bits
// ---------------------------------------------------------------------------

/// The native Windows "Open" dialog, filtered to .json.
pub fn pick_json_file() -> Option<PathBuf> {
    com_init();
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let filters = [
            COMDLG_FILTERSPEC { pszName: w!("Quote files (*.json)"), pszSpec: w!("*.json") },
            COMDLG_FILTERSPEC { pszName: w!("All files (*.*)"), pszSpec: w!("*.*") },
        ];
        dialog.SetFileTypes(&filters).ok()?;
        dialog.SetTitle(w!("Import quotes")).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM).ok()?;
        dialog.Show(None).ok()?; // Err = user cancelled.
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const c_void));
        path.map(PathBuf::from)
    }
}

pub fn open_folder(path: &Path) {
    let _ = Command::new("explorer.exe").arg(path).spawn();
}

/// Font folders Windows itself doesn't register. Office 365 downloads its
/// cloud fonts (including Gill Sans Nova) here, visible only to Office.
pub fn extra_font_dirs() -> Vec<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|l| vec![PathBuf::from(l).join(r"Microsoft\FontCache\4\CloudFonts")])
        .unwrap_or_default()
}

pub fn ui_font(kind: UiFont) -> Option<PathBuf> {
    let windir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts = windir.join("Fonts");
    let candidates: &[&str] = match kind {
        UiFont::Regular => &["segoeui.ttf", "tahoma.ttf", "arial.ttf"],
        UiFont::Semibold => &["seguisb.ttf", "segoeuib.ttf", "tahomabd.ttf", "arialbd.ttf"],
        UiFont::Light => &["segoeuil.ttf", "segoeui.ttf", "arial.ttf"],
        UiFont::ModernRegular => &["SegUIVar.ttf", "segoeui.ttf", "arial.ttf"],
        UiFont::ModernSemibold => &["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf"],
        UiFont::Symbols => &["seguisym.ttf", "segoeui.ttf"],
    };
    candidates.iter().map(|f| fonts.join(f)).find(|p| p.is_file())
}

/// The user's accent colour (Settings > Personalisation > Colours).
pub fn accent_color() -> Option<[u8; 3]> {
    let mut argb = 0u32;
    let mut opaque = BOOL(0);
    unsafe { DwmGetColorizationColor(&mut argb, &mut opaque) }.ok()?;
    Some([(argb >> 16) as u8, (argb >> 8) as u8, argb as u8])
}

// ---------------------------------------------------------------------------
// Window effects
// ---------------------------------------------------------------------------

/// Real OS-composited materials for the settings window:
/// * Aero skin  -> Acrylic ("transient window") backdrop: genuinely blurred,
///   translucent glass behind our painted frame - the Windows 7 look,
///   rebuilt on the documented Windows 11 API.
/// * Modern skin -> Mica, exactly like built-in Windows 11 apps.
///
/// Both need Windows 11 22H2+. Elsewhere the calls fail harmlessly and the
/// UI paints an opaque approximation instead.
pub struct WindowFx {
    hwnd: isize,
}

impl WindowFx {
    pub fn attach(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match cc.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(Self { hwnd: h.hwnd.get() }),
            _ => None,
        }
    }

    /// Returns true if a translucent backdrop is now active (so the UI
    /// should paint see-through surfaces).
    pub fn apply(&self, skin: Skin, dark: bool, glass: bool) -> bool {
        // Only trust the backdrop API on builds that really implement it:
        // some older builds (and compatibility layers) accept the call and
        // then draw nothing, which would leave the window see-through.
        let glass = glass && windows_build() >= 22621;
        let hwnd = HWND(self.hwnd as *mut c_void);
        unsafe {
            set_attr(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, BOOL::from(dark));
            set_attr(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
            let backdrop: DWM_SYSTEMBACKDROP_TYPE = match (glass, skin) {
                (false, _) => DWMSBT_NONE,
                (true, Skin::Aero) => DWMSBT_TRANSIENTWINDOW,
                (true, Skin::Modern) => DWMSBT_MAINWINDOW,
            };
            let backdrop_ok = set_attr(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, backdrop);
            let active = glass && backdrop_ok;
            // -1 margins = "sheet of glass": the backdrop shows through the
            // whole client area wherever we paint transparent pixels.
            // Without glass, a 1px margin keeps the OS drop shadow on the
            // borderless Aero window.
            let margins = if active {
                MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 }
            } else {
                MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 0, cyBottomHeight: 1 }
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
            active
        }
    }
}

/// The OS build number (22621 = Windows 11 22H2), or 0 if unknown.
fn windows_build() -> u32 {
    let mut buf = [0u16; 16];
    let mut size = std::mem::size_of_val(&buf) as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    }
    .is_ok();
    if !ok {
        return 0;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len]).trim().parse().unwrap_or(0)
}

unsafe fn set_attr<T>(hwnd: HWND, attr: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE, value: T) -> bool {
    unsafe {
        DwmSetWindowAttribute(hwnd, attr, &value as *const T as *const c_void, std::mem::size_of::<T>() as u32).is_ok()
    }
}
