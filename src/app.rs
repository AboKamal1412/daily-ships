//! Application state and behaviour: which ship is showing, the auto-close
//! timer, fades, window placement and the three buttons.

use std::time::{Duration, Instant};

use eframe::egui::{self, Context, Key, TextureHandle, TextureOptions, ViewportCommand};

use crate::platform;
use crate::ships::{self, Ship};
use crate::storage::{self, Rng, Store};
use crate::ui::{self, Action, ShipView};

/// How long the popup stays up without interaction.
const AUTO_CLOSE: Duration = Duration::from_secs(30);
const FADE_IN: Duration = Duration::from_millis(420);
const FADE_OUT: Duration = Duration::from_millis(200);
/// Fade used when "Next Ship" swaps the content.
const SWAP_FADE: Duration = Duration::from_millis(260);
/// Frames during which the window position is (re)applied; see `place_window`.
const PLACEMENT_PASSES: u8 = 3;

pub struct DailyShipsApp {
    ships: Vec<Ship>,
    /// Index into `ships`; `None` when there is nothing to show.
    current: Option<usize>,
    /// Message for the error card shown when `current` is `None`.
    error: String,
    store: Store,
    rng: Rng,
    texture: Option<TextureHandle>,

    appeared: Option<Instant>,
    deadline: Instant,
    content_since: Option<Instant>,
    closing_since: Option<Instant>,
    placement_passes: u8,
}

impl DailyShipsApp {
    /// Builds the app and picks the ship to show. `requested_ship` (from
    /// `--ship <id>`) overrides the random choice when it names a real ship.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        store: Store,
        requested_ship: Option<String>,
    ) -> Result<Self, String> {
        ui::install_fonts(&cc.egui_ctx)?;

        let (ships, error) = match ships::load() {
            Ok(ships) => (ships, String::new()),
            Err(err) => {
                storage::log_error(&err);
                (Vec::new(), err)
            }
        };

        let mut app = Self {
            ships,
            current: None,
            error,
            store,
            rng: Rng::new(),
            texture: None,
            appeared: None,
            deadline: Instant::now() + AUTO_CLOSE,
            content_since: None,
            closing_since: None,
            placement_passes: 0,
        };

        let requested = requested_ship.and_then(|id| {
            app.ships
                .iter()
                .position(|ship| ship.id.eq_ignore_ascii_case(&id))
        });
        match requested {
            Some(index) => {
                storage::record_shown(&app.ships[index], &mut app.store.state);
                app.current = Some(index);
            }
            None => {
                app.current = storage::pick_next(&app.ships, &mut app.store.state, &mut app.rng);
            }
        }
        if app.current.is_some() {
            app.store.save_best_effort();
            app.load_texture(&cc.egui_ctx);
        }
        Ok(app)
    }

    /// Decodes the current ship's photo (and only that one) into a texture.
    /// On failure the card shows its placeholder artwork instead.
    fn load_texture(&mut self, ctx: &Context) {
        self.texture = None; // frees the previous photo first
        let Some(ship) = self.current.map(|index| &self.ships[index]) else {
            return;
        };
        match ships::decode_image(ship) {
            Ok(image) => {
                self.texture = Some(ctx.load_texture("ship-photo", image, TextureOptions::LINEAR));
            }
            Err(err) => storage::log_error(&err),
        }
    }

    fn next_ship(&mut self, ctx: &Context, now: Instant) {
        let Some(index) = storage::pick_next(&self.ships, &mut self.store.state, &mut self.rng)
        else {
            return;
        };
        self.current = Some(index);
        self.store.save_best_effort();
        self.load_texture(ctx);
        self.deadline = now + AUTO_CLOSE; // restart the 30 second timer
        self.content_since = Some(now);
    }

    fn snooze_for_today(&mut self) {
        self.store.state.snooze_date = Some(platform::local_date());
        self.store.save_best_effort();
    }

    fn begin_close(&mut self, now: Instant) {
        if self.closing_since.is_none() {
            self.closing_since = Some(now);
        }
    }

    /// Pins the window to the bottom-right corner of the primary monitor's
    /// work area. `main` already supplies an initial position; this corrects
    /// it using the real scale factor once the window exists, and shrinks the
    /// whole card (via the zoom factor) on screens too small to hold it.
    /// Zoom changes take effect one frame later, hence several passes.
    fn place_window(&mut self, ctx: &Context) {
        if self.placement_passes >= PLACEMENT_PASSES {
            return;
        }
        self.placement_passes += 1;
        let Some(area) = platform::work_area() else {
            return;
        };

        let native = ctx.native_pixels_per_point().unwrap_or(1.0).max(0.25);
        let size = ui::window_size();
        let fit = ((area.bottom - area.top) / native / size.y)
            .min((area.right - area.left) / native / size.x);
        let zoom = fit.clamp(0.5, 1.0);
        if (ctx.zoom_factor() - zoom).abs() > 0.005 {
            ctx.set_zoom_factor(zoom);
            return; // position and size are applied once the zoom is active
        }
        if zoom < 1.0 {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        }
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(ui::corner_position(
            area,
            ctx.pixels_per_point(),
        )));
    }
}

impl eframe::App for DailyShipsApp {
    /// The window is transparent; only the card is painted.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let appeared = match self.appeared {
            Some(appeared) => appeared,
            None => {
                // The 30 seconds start when the popup is first drawn.
                self.appeared = Some(now);
                self.deadline = now + AUTO_CLOSE;
                now
            }
        };
        self.place_window(ctx);

        if ctx.input(|input| input.key_pressed(Key::Escape)) || now >= self.deadline {
            self.begin_close(now);
        }

        // Window fade in / fade out.
        let mut opacity = ease_out(progress(appeared, now, FADE_IN));
        if let Some(since) = self.closing_since {
            let t = progress(since, now, FADE_OUT);
            opacity *= 1.0 - t;
            if t >= 1.0 {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
        let content_progress = self
            .content_since
            .map_or(1.0, |since| progress(since, now, SWAP_FADE));

        let remaining = self.deadline.saturating_duration_since(now);
        let seconds_left = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);

        let action = match self.current {
            Some(index) => ui::show_ship(
                ctx,
                opacity,
                &ShipView {
                    ship: &self.ships[index],
                    texture: self.texture.as_ref(),
                    seconds_left,
                    content_opacity: ease_out(content_progress),
                    has_other_ships: self.ships.len() > 1,
                },
            ),
            None => ui::show_error(ctx, opacity, &self.error, seconds_left),
        };

        if self.closing_since.is_none() {
            match action {
                Action::None => {}
                Action::NextShip => self.next_ship(ctx, now),
                Action::Close => self.begin_close(now),
                Action::DontShowToday => {
                    self.snooze_for_today();
                    self.begin_close(now);
                }
            }
        }

        // Repaint scheduling. While something is animating we draw every
        // frame; otherwise the app sleeps until the countdown text changes
        // (once a second) or the user moves the mouse over the card.
        let animating = self.closing_since.is_some()
            || progress(appeared, now, FADE_IN) < 1.0
            || self
                .content_since
                .is_some_and(|since| progress(since, now, SWAP_FADE) < 1.0)
            || self.placement_passes < PLACEMENT_PASSES;
        if animating {
            ctx.request_repaint();
        } else {
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            let until_next_second = Duration::from_nanos(u64::from(remaining.subsec_nanos()));
            ctx.request_repaint_after(until_next_second + Duration::from_millis(4));
        }
    }
}

/// Fraction (0..=1) of `duration` elapsed between `start` and `now`.
fn progress(start: Instant, now: Instant, duration: Duration) -> f32 {
    (now.saturating_duration_since(start).as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
}

/// Cubic ease-out.
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
