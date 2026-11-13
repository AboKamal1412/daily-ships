# Daily Ships

A small Windows 11 desktop app that shows one randomly chosen historical ship
in a card in the bottom-right corner of the screen each time you log in, then
closes itself after 30 seconds and exits completely.

- One self-contained `daily-ships.exe`. 24 ships and their photographs are
  compiled in; the app never touches the network.
- No installer, no tray icon, no background process, no service.
- Written in Rust with egui/eframe.

## What it does

When started, the app:

1. Exits immediately if you clicked **Don't show today** earlier the same day.
2. Picks a ship at random. It never shows the same ship twice in a row, and it
   goes through all ships once before any ship comes round again.
3. Fades in a borderless, always-on-top card in the bottom-right corner with
   the ship's photo, name, nation, class, launch year, fate, a short
   description and a "did you know" fact.
4. Closes after 30 seconds, then the process ends.

| Button | Effect |
|---|---|
| **Next Ship** | Shows another ship straight away and restarts the 30 second timer. |
| **Close** | Closes the popup now. `Esc` does the same while the popup has focus. |
| **Don't show today** | Closes the popup and suppresses it until the next local calendar day. |

The popup does not take keyboard focus when it appears, so it will not swallow
what you are typing.

## Requirements

- Windows 11 x64 (Windows 10 should also work but is untested).
- To build: the Rust toolchain and the Microsoft C++ build tools.

## Quick start with the prebuilt executable

`dist\daily-ships.exe` is the release build produced from this source. It is
not stored in the Git repository (`dist/` is ignored), so after a fresh clone
build it yourself as described below. If you have it, run it from PowerShell
in the project folder:

```powershell
.\dist\daily-ships.exe
```

## Building from source

Open PowerShell in the project folder (the one containing `Cargo.toml`).

### 1. Install the dependencies (once)

Rust, via rustup:

```powershell
winget install --id Rustlang.Rustup -e
```

The Microsoft C++ build tools (linker and Windows SDK). Skip this if Visual
Studio with the "Desktop development with C++" workload is already installed:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Open a new PowerShell window afterwards so `cargo` is on the `PATH`. The Rust
crates themselves are downloaded automatically by the first build.

### 2. Run locally (debug build, with a console for diagnostics)

```powershell
cargo run
```

### 3. Run the tests

```powershell
cargo test
```

### 4. Build the release executable

```powershell
cargo build --release
```

The Windows executable is written to `target\release\daily-ships.exe`. Run it:

```powershell
.\target\release\daily-ships.exe
```

### Command-line options

All optional; mainly useful for trying things out.

| Option | Meaning |
|---|---|
| `--ship <id>` | Show a specific ship, for example `--ship vasa`. Ids are in `assets/ships.json`. |
| `--force` | Show the popup even if **Don't show today** was clicked today. |
| `--delay <seconds>` | Wait before doing anything (maximum 600). Used by the Run-key fallback below. |

```powershell
.\target\release\daily-ships.exe --force --ship cutty-sark
```

## Start at login

The app never changes Windows start-up settings by itself. Two scripts do it,
and only when you run them.

### Install

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1
```

This copies the executable to `%LOCALAPPDATA%\Programs\DailyShips\` and
registers a scheduled task named **Daily Ships** with these properties:

- Trigger: at log on of the current user, delayed by 20 seconds.
- Runs only for the current user, only while logged on, without elevation.
- The task just starts the app; nothing stays running after the popup closes.

Useful variations:

```powershell
# Preview what would happen without changing anything
powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1 -WhatIf

# Different delay
powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1 -DelaySeconds 45

# Try the task immediately instead of logging out and in
Start-ScheduledTask -TaskName 'Daily Ships'
```

After rebuilding, run the install script again to update the installed copy.

### If Windows refuses to create the task

Some Windows configurations only allow administrators to create log-on
triggered tasks, and the script then stops with an "access denied" message.
Either run the same command from an elevated PowerShell (the task is still
created for your account only and still runs without administrator rights), or
use the fallback that needs no special rights:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install-startup.ps1 -UseRunKey
```

That adds a per-user `Run` registry entry which starts the app with
`--delay 20`. The app then sleeps for the delay itself before showing the
popup, and still exits afterwards.

### Uninstall

```powershell
powershell -ExecutionPolicy Bypass -File scripts\uninstall-startup.ps1
```

To also delete the installed copy and the saved state:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\uninstall-startup.ps1 -RemoveFiles -RemoveData
```

## Where things are stored

| Path | Contents |
|---|---|
| `%LOCALAPPDATA%\DailyShips\state.json` | Last ship shown, ships already seen in the current cycle, and the "don't show today" date. |
| `%LOCALAPPDATA%\DailyShips\error.log` | Created only if something goes wrong (the release build has no console). |
| `%LOCALAPPDATA%\Programs\DailyShips\` | The copy of the executable made by the install script. |

Deleting `state.json` is always safe. A missing, unreadable or corrupted state
file is treated as "start fresh".

## Project layout

```
daily-ships/
├── Cargo.toml              dependencies and the size-optimised release profile
├── Cargo.lock
├── build.rs                embeds every JPEG in assets/images into the executable
├── src/
│   ├── main.rs             arguments, "don't show today" check, window options
│   ├── app.rs              state, timer, fades, window placement, button actions
│   ├── ui.rs               fonts, colours, card layout, custom buttons
│   ├── ships.rs            ship database parsing and image decoding
│   ├── storage.rs          state file, random selection, error log
│   └── platform.rs         the four Win32 calls the app needs
├── assets/
│   ├── ships.json          the ship database (24 ships)
│   └── images/             one 800 px JPEG per ship
├── scripts/
│   ├── install-startup.ps1
│   └── uninstall-startup.ps1
├── dist/daily-ships.exe    prebuilt release executable
├── LICENSES.md             image credits and licences
└── README.md
```

`platform.rs` is an addition to the usual layout: it keeps the `unsafe` Win32
declarations (local date, desktop work area, DPI awareness) in one small file
so that no extra crate is needed for them. `winreg` is not used because the
app itself never reads or writes the registry.

## Editing the ship database

`assets/ships.json` is an array of objects:

```json
{
  "id": "vasa",
  "name": "Vasa",
  "nation": "Sweden",
  "class": "64-gun warship",
  "launched": 1627,
  "fate": "Sank in Stockholm harbour on 10 August 1628; raised in 1961",
  "description": "...",
  "fun_fact": "...",
  "image": "vasa.jpg",
  "image_credit": "Photo: JavierKohen · CC BY-SA 3.0",
  "focus": [0.5, 0.4]
}
```

- `launched` is a year, or text such as `"Unknown (15th century)"`.
- `image` is a file name in `assets/images/`. Only JPEG is supported.
- `image_credit` (optional) is printed on the photo.
- `focus` (optional, default `[0.5, 0.5]`) is the point of the photo, as x and
  y fractions, to keep in view when the photo is cropped to the card.

The card has a fixed size. `cargo test` enforces text lengths that are known
to fit (description up to 230 characters, fun fact up to 180, fate up to 80).
If a text runs long anyway, the photo gives up height to make room.

Rebuild after any change; the database and images are compiled in. See
`LICENSES.md` for how to replace images properly.

## How failures are handled

| Situation | Behaviour |
|---|---|
| Image missing from the build, or not a valid JPEG | The card shows placeholder artwork instead of the photo; details go to `error.log`. |
| `ships.json` invalid or empty | Cannot normally happen in a released build because `cargo test` checks it; if it does, a card saying so is shown and closes like any other. |
| State file missing, corrupted or unwritable | Treated as a fresh start; the popup still appears. |
| Window cannot be created (for example no usable graphics driver) | The app writes the reason to `error.log` and exits. |
| Scheduled task cannot be registered | The install script explains why and what to do; nothing is left half-installed. |

## Measurements

Measured on the machine this was built on (Windows 11 Pro 26200, AMD Radeon
RX 9070 XT, display at 100 % scaling), release build, five runs each. They are
measurements of one machine, not guarantees.

| What | Result |
|---|---|
| Executable size | 3,529,728 bytes (3.37 MB), of which about 1.7 MB is the 24 photographs |
| Process start to visible window (warm start) | about 150 ms (median 157 ms; one of ten runs took 241 ms) |
| Start to exit when "don't show today" is active | about 0.1 s, no window created |
| Working set while the popup is visible | about 96 MB |
| Private bytes while the popup is visible | about 204 MB |
| Threads while the popup is visible | 13 |
| CPU time over 10 idle seconds with the popup open | 0 ms (below the timer's resolution) |
| Auto-close | 30.6 s after appearing, including the fade-out |
| After closing | no process remains |

Notes on these numbers:

- The memory figure is larger than the app's own data would suggest. The app
  holds one decoded photo (about 2 MB) and a few system fonts; most of the rest,
  and nearly all of the threads, belong to the graphics driver's OpenGL
  runtime, which eframe needs to draw. It is held for the 30 seconds the popup
  is visible and then released. A different GPU or driver will give different
  numbers.
- The start-up time is for a warm start. The first start after boot, 20
  seconds into a login, will be slower because the executable and graphics
  driver are not yet cached; that was not measured.
- Only the selected ship's image is decoded. The popup repaints once a second
  for the countdown, and when the mouse moves over it; there is no polling.

Reproduce the size check:

```powershell
(Get-Item .\target\release\daily-ships.exe).Length
```

## What was and was not tested

Tested on the build machine: compilation, the 14 unit tests, the popup's
appearance for several ships, position in the bottom-right corner, **Next
Ship** (including no immediate repeats), **Close**, **Don't show today**
(including the silent exit on the next start), recovery from a corrupted state
file, the 30 second auto-close, and that the popup does not take focus.

Not tested: actually registering the scheduled task. The two scripts were
checked for syntax and run with `-WhatIf` only, so that the build did not
change this PC's start-up configuration. Display scaling other than 100 %,
the scale-down path for small screens, multiple monitors and Windows 10 were
also not tested.

## Windows-specific limitations

- **Primary monitor only.** The popup uses the work area of the primary
  display.
- **Graphics.** eframe draws with OpenGL. Machines without a working OpenGL
  driver (some virtual machines and remote sessions) cannot show the popup; the
  reason is written to `error.log`.
- **Small screens.** If the work area is too small for the 452 × 660 window,
  the whole card is scaled down to fit (to 50 % at most).
- **Fonts.** Segoe UI and Georgia are loaded from `C:\Windows\Fonts`. Both ship
  with Windows 11. If Georgia is missing, Segoe UI is used for everything.
- **Always on top.** The popup stays above normal windows, but exclusive
  full-screen games and the lock screen still cover it. It does not follow
  Windows "Do not disturb".
- **Unsigned executable.** If you copy the exe to another PC, SmartScreen may
  warn about an unrecognised app, as it does for any unsigned program.
- **Long folder paths.** The Microsoft linker fails with `LNK1104` when the
  project sits in a very deep folder, because paths inside `target\` exceed 260
  characters. Move the project to a short path such as `C:\dev\daily-ships`, or
  redirect the build output first:

  ```powershell
  $env:CARGO_TARGET_DIR = "C:\cargo-target\daily-ships"
  ```

  The executable is then at `C:\cargo-target\daily-ships\release\daily-ships.exe`;
  pass that to the install script with `-ExePath`.
- **Task Scheduler rights.** See "If Windows refuses to create the task".

## Design notes

- **Binary size.** `opt-level = "z"`, LTO, one codegen unit, stripped symbols
  and `panic = "abort"`. eframe is built without its default features (no
  accessibility layer, no bundled fonts, no persistence, OpenGL renderer only),
  and `image` with only its JPEG decoder.
- **No crate where a few lines do.** Randomness comes from the standard
  library's OS-seeded hasher plus SplitMix64; the local date and work area come
  from direct Win32 declarations.
- **One thread of our own, no timers of our own.** The 30 second close and the
  countdown are driven by egui's scheduled repaints.
- **Icon.** The executable has no custom icon. The window has no title bar or
  taskbar button, so it would only show in Explorer and Task Manager.

## Licences

Photographs: see [LICENSES.md](LICENSES.md). Eight of the 24 are under Creative
Commons attribution licences and must stay credited.
