//! The few Win32 calls the app needs, declared directly so no extra crate is
//! required: the local calendar date, the desktop work area, DPI awareness and
//! detection of full-screen applications.

use std::ffi::c_void;

#[repr(C)]
#[derive(Default)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy, PartialEq, Debug)]
struct RawRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Default)]
struct MonitorInfo {
    size: u32,
    monitor: RawRect,
    work: RawRect,
    flags: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(system_time: *mut SystemTime);
}

#[link(name = "user32")]
extern "system" {
    fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, win_ini: u32) -> i32;
    fn SetProcessDpiAwarenessContext(context: isize) -> i32;
    fn GetDpiForSystem() -> u32;
    fn GetForegroundWindow() -> isize;
    fn GetShellWindow() -> isize;
    fn GetDesktopWindow() -> isize;
    fn GetWindowRect(window: isize, rect: *mut RawRect) -> i32;
    fn GetClassNameW(window: isize, buffer: *mut u16, capacity: i32) -> i32;
    fn MonitorFromWindow(window: isize, flags: u32) -> isize;
    fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn SHQueryUserNotificationState(state: *mut i32) -> i32;
}

const MONITOR_DEFAULTTONEAREST: u32 = 2;

// QUERY_USER_NOTIFICATION_STATE values that mean "do not interrupt the user".
const QUNS_NOT_PRESENT: i32 = 1; // locked, screen saver or fast user switching
const QUNS_BUSY: i32 = 2; // a full-screen application is running
const QUNS_RUNNING_D3D_FULL_SCREEN: i32 = 3; // exclusive full-screen Direct3D (games)
const QUNS_PRESENTATION_MODE: i32 = 4; // presentation settings are on

const SPI_GETWORKAREA: u32 = 0x0030;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

/// The primary monitor's work area (the desktop minus the taskbar), in
/// physical pixels once the process is DPI aware.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkArea {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Today's date in the user's local time zone, as `YYYY-MM-DD`.
pub fn local_date() -> String {
    let mut now = SystemTime::default();
    // SAFETY: GetLocalTime only writes to the struct we pass and cannot fail.
    unsafe { GetLocalTime(&mut now) };
    format!("{:04}-{:02}-{:02}", now.year, now.month, now.day)
}

/// Opts in to per-monitor DPI awareness so that coordinates are real pixels.
/// Must run before any window exists. Failure is harmless (winit also tries).
pub fn enable_dpi_awareness() {
    // SAFETY: plain call with a documented constant.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// Scale factor of the primary monitor (1.0 = 100 %, 1.5 = 150 %, ...).
pub fn system_scale() -> f32 {
    // SAFETY: no arguments, no failure mode.
    let dpi = unsafe { GetDpiForSystem() };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

/// Work area of the primary monitor, or `None` if Windows refuses to say.
pub fn work_area() -> Option<WorkArea> {
    let mut rect = RawRect::default();
    // SAFETY: SPI_GETWORKAREA writes one RECT to the pointer we provide.
    let ok =
        unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut rect as *mut RawRect).cast(), 0) };
    if ok == 0 || rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }
    Some(WorkArea {
        left: rect.left as f32,
        top: rect.top as f32,
        right: rect.right as f32,
        bottom: rect.bottom as f32,
    })
}

/// True when the popup should stay away: a game, video or presentation is
/// running full screen, or nobody is at the screen (locked / screen saver).
///
/// Two checks are combined. Windows' own answer to "may I show a
/// notification now?" covers exclusive full-screen games and presentation
/// mode. Comparing the foreground window with its monitor additionally
/// catches borderless full-screen windows (most modern games, F11 in a
/// browser, full-screen video players).
pub fn fullscreen_app_active() -> bool {
    let mut state = 0i32;
    // SAFETY: writes one i32 to the pointer we provide; returns an HRESULT.
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    if hr >= 0 && notification_state_blocks(state) {
        return true;
    }
    foreground_window_fills_monitor()
}

fn notification_state_blocks(state: i32) -> bool {
    matches!(
        state,
        QUNS_NOT_PRESENT | QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE
    )
}

fn foreground_window_fills_monitor() -> bool {
    // SAFETY: all calls take plain handles and pointers to structs we own;
    // each failure is checked and simply means "not full screen".
    unsafe {
        let window = GetForegroundWindow();
        if window == 0 || window == GetShellWindow() || window == GetDesktopWindow() {
            return false;
        }
        // The desktop itself (wallpaper and icons) covers the whole monitor.
        let mut class = [0u16; 64];
        let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        let class = String::from_utf16_lossy(&class[..len.clamp(0, 63) as usize]);
        if class == "Progman" || class == "WorkerW" {
            return false;
        }
        let mut rect = RawRect::default();
        if GetWindowRect(window, &mut rect) == 0 {
            return false;
        }
        let mut info = MonitorInfo {
            size: std::mem::size_of::<MonitorInfo>() as u32,
            ..Default::default()
        };
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        if monitor == 0 || GetMonitorInfoW(monitor, &mut info) == 0 {
            return false;
        }
        covers_exactly(rect, info.monitor)
    }
}

/// A full-screen window has exactly the monitor's rectangle. A maximised
/// window does not: it leaves the taskbar free, or (with an auto-hidden
/// taskbar) overshoots every edge by the width of its invisible frame.
fn covers_exactly(window: RawRect, monitor: RawRect) -> bool {
    monitor.right > monitor.left && monitor.bottom > monitor.top && window == monitor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_busy_states_block_the_popup() {
        for state in [1, 2, 3, 4] {
            assert!(notification_state_blocks(state), "state {state}");
        }
        // 5 = accepts notifications, 6 = quiet time, 7 = store app, 0 = unknown
        for state in [0, 5, 6, 7, 99] {
            assert!(!notification_state_blocks(state), "state {state}");
        }
    }

    #[test]
    fn maximised_windows_are_not_full_screen() {
        let rect = |left, top, right, bottom| RawRect {
            left,
            top,
            right,
            bottom,
        };
        let monitor = rect(0, 0, 2560, 1440);
        assert!(covers_exactly(rect(0, 0, 2560, 1440), monitor));
        // Maximised above a taskbar, and maximised with an auto-hidden taskbar.
        assert!(!covers_exactly(rect(-8, -8, 2568, 1400), monitor));
        assert!(!covers_exactly(rect(-8, -8, 2568, 1448), monitor));
        assert!(!covers_exactly(rect(100, 100, 900, 700), monitor));
        assert!(!covers_exactly(rect(0, 0, 0, 0), rect(0, 0, 0, 0)));
    }

    #[test]
    fn local_date_is_iso_formatted() {
        let date = local_date();
        assert_eq!(date.len(), 10);
        assert_eq!(date.as_bytes()[4], b'-');
        assert_eq!(date.as_bytes()[7], b'-');
        assert!(date[..4].parse::<u32>().unwrap() >= 2024);
    }

    #[test]
    fn work_area_is_sane() {
        let area = work_area().expect("a desktop session has a work area");
        assert!(area.right - area.left >= 320.0);
        assert!(area.bottom - area.top >= 200.0);
    }
}
