# Doomsday Clock

A tiny Windows utility that turns your desktop wallpaper into a daily
countdown to a date you pick — "47 Days left till 25-12-2026" in huge bold
type on a black background, updated automatically every day, with an option
to have the text fade from white to red as the deadline gets closer.

No installer, no background service eating RAM, no telemetry, no third-party
libraries. One portable `.exe`, one Windows Scheduled Task.

---

## Contents

- [What it does](#what-it-does)
- [How it works (architecture)](#how-it-works-architecture)
- [Building it yourself](#building-it-yourself)
- [Running it](#running-it)
- [The config file](#the-config-file)
- [UI design philosophy](#ui-design-philosophy)
- [How to change common things](#how-to-change-common-things)
- [Uninstalling](#uninstalling)
- [Why these engineering choices?](#why-these-engineering-choices)

---

## What it does

1. You open `DoomsdayClock.exe`, pick a target date, and choose whether the
   countdown text should stay white or gradually fade to red as the date
   approaches.
2. You click **Save & Activate**. The app immediately:
   - renders today's wallpaper (a black canvas with the countdown text,
     auto-sized to fill the screen) and sets it as your desktop background,
     and
   - installs a per-user **Windows Scheduled Task** that silently re-runs
     the same rendering step once a day (at 00:05, and again at every
     logon, so a machine that was asleep at 00:05 still catches up).
3. Every day, without the app's window ever opening, your wallpaper
   updates itself to show the new day count.

No process sits resident in memory between runs — the Scheduled Task
launches the `.exe` for a fraction of a second, it draws one image, sets it,
and exits. This is the leanest possible way to do "runs every day in the
background" on Windows.

## How it works (architecture)

```
                    ┌───────────────────────┐
   You double-click │      Program.cs        │  no args
   the .exe    ───► │  (routes by argv[0])   │ ───────────────► SettingsForm (UI)
                    └───────────┬────────────┘
                                 │ --update
                                 ▼
                    ┌───────────────────────┐
   Scheduled Task    │   WallpaperSetter      │
   runs this daily ► │  .ApplyForToday(...)   │
                    └───────────┬────────────┘
                                 │
                 ┌───────────────┴────────────────┐
                 ▼                                 ▼
      ┌─────────────────────┐          ┌─────────────────────────┐
      │  WallpaperRenderer    │          │  Native SystemParameters  │
      │  (GDI+ draws the       │ Bitmap  │  Info(SPI_SETDESKWALLPAPER)│
      │  black canvas + text)  │────────►│  + registry WallpaperStyle │
      └──────────┬──────────┘          └─────────────────────────┘
                 │ uses
        ┌────────┴────────┐
        ▼                 ▼
 FontResolver     ColorInterpolator
 (font fallback)   (white → red math)
```

| File | Responsibility |
|---|---|
| `Program.cs` | Entry point. Dispatches to the UI, the silent updater, or `--uninstall` based on the command-line argument. This is the **only** file that knows both "modes" exist. |
| `AppConfig.cs` | The settings model (target date, colour mode, font) plus JSON load/save to `%AppData%\DoomsdayClock\config.json`. The single source of truth both the UI and the silent updater read. |
| `WallpaperRenderer.cs` | Pure rendering: given a config and a date, produces a `Bitmap`. Knows nothing about files, the registry, or Windows APIs — this is what makes it reusable for both the real wallpaper *and* the UI's live preview. |
| `ColorInterpolator.cs` | Isolated, allocation-free white→red colour math. No GDI+, no I/O — easy to reason about on its own. |
| `FontResolver.cs` | Resolves "Gill Sans Nova Ultra Bold" to an installed font, walking a fallback chain if it isn't present on the machine. |
| `WallpaperSetter.cs` | Glue: calls the renderer, writes the `.bmp`, and calls into `Native/NativeMethods.cs` + the registry to actually apply it as wallpaper. |
| `TaskSchedulerService.cs` | Installs/queries/removes the daily Scheduled Task via `schtasks.exe`. |
| `Native/NativeMethods.cs` | Every raw Win32 P/Invoke signature in the app, in one auditable place. |
| `UI/SettingsForm.cs` | The one window. Wires up the controls, calls `AppConfig`, `WallpaperSetter`, and `TaskSchedulerService`. |
| `UI/GlassButton.cs`, `UI/GlassPanel.cs`, `UI/AeroPalette.cs` | The hand-painted "Aero glass" custom controls and shared palette/gradient helpers — see [UI design philosophy](#ui-design-philosophy). |

## Building it yourself

You need the [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0)
on **Windows** (WinForms projects can only be fully restored/compiled on
Windows, since the Windows Desktop reference assemblies are Windows-only).

```powershell
# Clone, then from the repo root:
dotnet build src\DoomsdayClock\DoomsdayClock.csproj -c Release
```

To produce the portable, double-click-and-run `.exe` (recommended - what
the [Releases](../../releases) page ships):

```powershell
dotnet publish src\DoomsdayClock\DoomsdayClock.csproj `
  -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -o publish
```

This produces `publish\DoomsdayClock.exe` — a single file, roughly 70-150 MB,
that runs on a bare Windows 10/11 machine with **no** .NET runtime installed.

If you already have the .NET 8 Desktop Runtime installed (or don't mind
installing it once) and would rather have a ~150 KB exe instead of a
bundled one, publish framework-dependent instead:

```powershell
dotnet publish src\DoomsdayClock\DoomsdayClock.csproj `
  -c Release -r win-x64 --self-contained false -p:PublishSingleFile=true -o publish
```

Every push to `main` and every tagged release (`vX.Y.Z`) is also built
automatically by GitHub Actions — see `.github/workflows/build.yml` and
`.github/workflows/release.yml`. Tagging a release (`git tag v1.0.0 && git
push --tags`) automatically attaches a built `DoomsdayClock.exe` to a GitHub
Release.

## Running it

- **Double-click `DoomsdayClock.exe`** → opens the settings window.
- **`DoomsdayClock.exe --update`** → silently regenerates and applies
  today's wallpaper, no window. This is what the Scheduled Task runs; you
  can also run it yourself to force an immediate refresh (e.g. after
  changing your monitor resolution).
- **`DoomsdayClock.exe --uninstall`** → removes the Scheduled Task and
  deletes the saved config. Equivalent to clicking "Disable Updates" in the
  UI, but scriptable.

No installer is provided or needed - it's one file, it writes nothing
outside `%AppData%\DoomsdayClock\` and `%LocalAppData%\DoomsdayClock\`, and
it makes no registry changes beyond the two standard wallpaper keys every
"set wallpaper" tool touches.

## The config file

`%AppData%\DoomsdayClock\config.json`:

```json
{
  "TargetDate": "2026-12-25T00:00:00",
  "CreatedDate": "2026-09-13T00:00:00",
  "ColorMode": "GradientToRed",
  "FontFamilyName": "Gill Sans Nova Ultra Bold"
}
```

- **`TargetDate`** — the deadline being counted down to.
- **`CreatedDate`** — the day this particular countdown was configured.
  This is "day zero" for the red-fade gradient: the text is pure white at
  `CreatedDate` and pure red at `TargetDate`, linearly interpolated in
  between. It resets automatically whenever you pick a *new* target date
  in the UI, so the fade always spans the full life of whatever countdown
  is currently active.
- **`ColorMode`** — `"StaticWhite"` or `"GradientToRed"`.
- **`FontFamilyName`** — the font the renderer tries first. Change this
  (or just install the actual "Gill Sans Nova Ultra Bold" font) to use a
  different typeface without touching code.

You can hand-edit this file; the next `--update` run (or reopening the UI)
picks up the change. Deleting it resets the app to "not configured" - the
Scheduled Task will simply do nothing until you open the UI and save again.

## UI design philosophy

The settings window is deliberately **not** a flat, minimal, modern-Fluent
form. It's a hand-painted homage to the Windows 7 "Aero" era: gradient glass
panels, glossy rounded buttons with a highlight bubble, a draggable
gradient title bar, and a soft OS-drawn drop shadow around a rounded
window.

**Why hand-painted instead of real DWM glass/blur?** True Aero glass
(`DwmEnableBlurBehindWindow`) was a Windows Vista/7-only compositor effect
that Microsoft removed starting with Windows 8, and its unofficial
Windows 10/11 replacement (`SetWindowCompositionAttribute`/Acrylic) is an
**undocumented** API that can change or break without notice between
Windows builds. Depending on it would make the UI fragile across OS
versions for a purely cosmetic effect. Hand-painting the *look* of glass
with GDI+ gradients and gloss overlays gets the same visual language, costs
almost nothing to render (a handful of `LinearGradientBrush`/
`PathGradientBrush` fills, cached where possible), and works identically
on every Windows version from 8.1 through 11 - "modernizing and
revitalizing" the aesthetic without depending on the deprecated
implementation.

- `UI/AeroPalette.cs` - all shared colours, fonts, and the two reusable
  paint helpers (`RoundedRect`, `PaintGlossHighlight`) every custom control
  uses. Change the palette here to re-theme the whole app in one place.
- `UI/GlassButton.cs` - a `Control` subclass (not a themed `Button`) so its
  paint isn't fought over by Windows' visual-styles engine. Has hover/press
  states and four accent colours (`Blue`, `Green`, `Red`, `Neutral`).
- `UI/GlassPanel.cs` - the sunken "well" grouping container with an
  optional title, used for every section of the form.
- `UI/SettingsForm.cs` - composes the above into the actual window,
  including the borderless-window drag handling and rounded-region/
  drop-shadow setup.

## How to change common things

| I want to... | Change this |
|---|---|
| Use a different font | Edit `FontFamilyName` in a saved `config.json`, or change the default in `AppConfig.cs` and the fallback chain in `FontResolver.cs`. |
| Change the red-fade colour | `ColorInterpolator.Red` (and `.White`) in `ColorInterpolator.cs`. |
| Change the daily run time | `TaskSchedulerService.DailyRunTime` (a `"HH:mm"` string). |
| Change the wallpaper text format | `WallpaperRenderer.BuildCountdownText(...)`. |
| Re-theme the whole UI | `UI/AeroPalette.cs` - every colour used anywhere in the window lives there. |
| Change the window size/layout | `UI/SettingsForm.cs` constructor - layout is explicit `Bounds`, top to bottom, no layout-engine magic to fight with. |
| Add a new setting | Add a property to `AppConfig`, a control to `SettingsForm`, and read/write it in `BuildConfigFromControls()`. |

## Uninstalling

Click **Disable Updates** in the app (removes the Scheduled Task), or run:

```powershell
DoomsdayClock.exe --uninstall
```

then delete the `.exe` itself and, if you want to remove the last-applied
wallpaper image too, `%LocalAppData%\DoomsdayClock\`.

## Why these engineering choices?

A few decisions that might look surprising at first glance, explained:

- **One `.exe`, two modes (`--update` / UI), instead of two separate
  binaries.** A portable single-file app should stay a single file. The
  Scheduled Task action is just `DoomsdayClock.exe --update` - nothing else
  to keep track of on disk.
- **`schtasks.exe` instead of a Task Scheduler NuGet library.** Zero extra
  dependency, zero extra published bytes, and the interaction happens
  exactly once per "Save" click - process-launch overhead is irrelevant at
  that frequency, and it avoids COM interop's STA/RCW lifetime footguns for
  a one-shot call.
- **BMP, not PNG, for the cached wallpaper file.** `SPI_SETDESKWALLPAPER`
  has guaranteed BMP support back to Windows XP. The file lives entirely in
  our own AppData folder, so its larger size costs nothing.
- **No `PublishTrimmed`.** WinForms leans on reflection-based
  designer/serialization plumbing that doesn't trim safely yet. Chasing a
  smaller binary at the risk of a runtime crash is the wrong trade for a
  utility that has to work unattended, every day, without anyone watching.
- **`InvariantGlobalization` is on.** The app only ever formats dates as
  `dd-MM-yyyy`, manually, itself - it never needs the OS's regional ICU
  data, so shipping it would be pure waste.
- **No third-party NuGet packages**, aside from Microsoft's own
  `Microsoft.Win32.Registry` (a first-party BCL package, not bundled with
  every target framework by default). Fewer dependencies means less
  supply-chain surface, a faster restore, and a smaller binary.
