//! Daily Ships: shows one random historical ship in a small popup in the
//! bottom-right corner of the screen, then exits.
//!
//! The process does nothing after the window closes: no tray icon, no
//! background service, no network access. Start-up at login is configured by
//! `scripts/install-startup.ps1`, never by the app itself.
//!
//! Command line (all optional):
//!   --ship <id>      show a specific ship (ids are listed in assets/ships.json)
//!   --force          show the popup even after "Don't show today"
//!   --delay <secs>   wait before doing anything (used by the Run-key
//!                    fallback of the install script; capped at 10 minutes)

// No console window in release builds. Debug builds keep it for diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Daily Ships is a Windows-only application.");

mod app;
mod platform;
mod ships;
mod storage;
mod ui;

use std::time::Duration;

use eframe::egui::ViewportBuilder;

const APP_NAME: &str = "Daily Ships";
const MAX_DELAY_SECS: u64 = 600;

#[derive(Default)]
struct Args {
    ship: Option<String>,
    force: bool,
    delay: Option<Duration>,
}

impl Args {
    /// Unknown arguments are ignored: a popup should never fail over a typo.
    fn parse(mut args: impl Iterator<Item = String>) -> Self {
        let mut parsed = Self::default();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--force" => parsed.force = true,
                "--ship" => parsed.ship = args.next().filter(|id| !id.is_empty()),
                "--delay" => {
                    parsed.delay = args
                        .next()
                        .and_then(|secs| secs.parse::<u64>().ok())
                        .map(|secs| Duration::from_secs(secs.min(MAX_DELAY_SECS)));
                }
                _ => {}
            }
        }
        parsed
    }
}

fn main() {
    let args = Args::parse(std::env::args().skip(1));
    if let Some(delay) = args.delay {
        std::thread::sleep(delay);
    }

    // "Don't show today": exit before creating any window. Asking for a
    // specific ship counts as wanting to see it.
    let store = storage::Store::load();
    if !args.force && args.ship.is_none() && store.is_snoozed(&platform::local_date()) {
        return;
    }

    platform::enable_dpi_awareness();

    let mut viewport = ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size(ui::window_size())
        .with_decorations(false) // borderless
        .with_transparent(true) // rounded corners and fades are painted by us
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(false); // do not steal keyboard focus at login
    if let Some(area) = platform::work_area() {
        viewport = viewport.with_position(ui::corner_position(area, platform::system_scale()));
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: false,
        ..Default::default()
    };

    let requested_ship = args.ship;
    let result = eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            let app = app::DailyShipsApp::new(cc, store, requested_ship)?;
            Ok(Box::new(app))
        }),
    );
    if let Err(err) = result {
        storage::log_error(&format!("the window could not be created: {err}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Args {
        Args::parse(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn arguments_are_parsed_leniently() {
        let args = parse(&["--ship", "vasa", "--force", "--delay", "20", "--unknown"]);
        assert_eq!(args.ship.as_deref(), Some("vasa"));
        assert!(args.force);
        assert_eq!(args.delay, Some(Duration::from_secs(20)));

        assert_eq!(
            parse(&["--delay", "99999"]).delay,
            Some(Duration::from_secs(MAX_DELAY_SECS))
        );
        assert_eq!(parse(&["--delay", "soon"]).delay, None);
        assert!(parse(&["--ship"]).ship.is_none());
        assert!(!parse(&[]).force);
    }
}
