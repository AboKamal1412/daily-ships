//! The few Win32 calls the app needs, declared directly so no extra crate is
//! required: the local calendar date, the desktop work area and DPI awareness.

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
#[derive(Default)]
struct RawRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
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
}

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

#[cfg(test)]
mod tests {
    use super::*;

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
