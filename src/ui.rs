//! Everything visual: fonts, colours, card layout and the custom buttons.
//!
//! The window itself is transparent and borderless; the card (rounded
//! corners, drop shadow) is painted by us inside it.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{
    self, epaint::Shadow, load::SizedTexture, pos2, vec2, Align, CentralPanel, Color32, Context,
    CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Frame, Galley, Image,
    Label, Layout, Mesh, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind,
    TextStyle, TextWrapMode, TextureHandle, Ui, UiBuilder, Vec2, WidgetText,
};

use crate::platform::WorkArea;
use crate::ships::Ship;

// ---------------------------------------------------------------- geometry

pub const CARD_W: f32 = 420.0;
pub const CARD_H: f32 = 628.0;
/// Transparent border around the card that holds the drop shadow. It also
/// provides the gap between the card and the screen edges.
pub const SHADOW_MARGIN: f32 = 16.0;

const RADIUS: u8 = 14;
const PAD: f32 = 24.0;
const PHOTO_MIN_H: f32 = 190.0;
const PHOTO_MAX_H: f32 = 236.0;
/// The photo never shrinks below this, however long the text is.
const PHOTO_FLOOR_H: f32 = 150.0;
const BODY_TOP_GAP: f32 = 12.0;
const BODY_BOTTOM_GAP: f32 = 18.0;
const BUTTON_H: f32 = 32.0;
const FOOTER_BOTTOM_PAD: f32 = 20.0;
const FACT_LABEL_W: f32 = 84.0;

/// Logical size of the (transparent) window.
pub fn window_size() -> Vec2 {
    vec2(CARD_W + 2.0 * SHADOW_MARGIN, CARD_H + 2.0 * SHADOW_MARGIN)
}

/// Top-left window position (in logical points) that puts the window in the
/// bottom-right corner of `area`, which is given in physical pixels.
pub fn corner_position(area: WorkArea, pixels_per_point: f32) -> Pos2 {
    let ppp = pixels_per_point.max(0.25);
    let size = window_size();
    pos2(area.right / ppp - size.x, area.bottom / ppp - size.y)
}

// ------------------------------------------------------------------ colours

const NAVY: Color32 = Color32::from_rgb(11, 20, 36);
const NAVY_RAISED: Color32 = Color32::from_rgb(20, 32, 54);
const GOLD: Color32 = Color32::from_rgb(201, 169, 97);
const GOLD_BRIGHT: Color32 = Color32::from_rgb(222, 193, 126);
const GOLD_DIM: Color32 = Color32::from_rgb(150, 128, 80);
const CREAM: Color32 = Color32::from_rgb(244, 238, 226);
const TEXT: Color32 = Color32::from_rgb(196, 205, 220);
const MUTED: Color32 = Color32::from_rgb(124, 137, 160);

// -------------------------------------------------------------------- fonts

const FAMILY_SEMIBOLD: &str = "semibold";
const FAMILY_SERIF: &str = "serif";
const FAMILY_SERIF_ITALIC: &str = "serif-italic";

fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_SEMIBOLD.into()))
}
fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_SERIF.into()))
}
fn serif_italic(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_SERIF_ITALIC.into()))
}

/// Loads the typefaces from the Windows font folder instead of embedding
/// them, which keeps roughly a megabyte out of the executable. Segoe UI and
/// Georgia ship with every Windows 11 installation; each role still has
/// fallbacks, and only a system with none of the body fonts is an error.
pub fn install_fonts(ctx: &Context) -> Result<(), String> {
    let dir = std::env::var_os("WINDIR")
        .or_else(|| std::env::var_os("SystemRoot"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");
    let load = |files: &[&str]| {
        files
            .iter()
            .find_map(|file| std::fs::read(dir.join(file)).ok())
    };

    let body_bytes = load(&["segoeui.ttf", "arial.ttf", "tahoma.ttf", "verdana.ttf"])
        .ok_or_else(|| format!("no usable system font found in {}", dir.display()))?;

    let mut fonts = FontDefinitions::empty();
    fonts.font_data.insert(
        "body".to_owned(),
        Arc::new(FontData::from_owned(body_bytes)),
    );
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["body".to_owned()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["body".to_owned()]);

    let roles: [(&str, &[&str]); 3] = [
        (
            FAMILY_SEMIBOLD,
            &["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf"],
        ),
        (
            FAMILY_SERIF,
            &["georgiab.ttf", "georgia.ttf", "timesbd.ttf", "cambriab.ttf"],
        ),
        (FAMILY_SERIF_ITALIC, &["georgiai.ttf", "timesi.ttf"]),
    ];
    for (family, files) in roles {
        let mut chain = Vec::with_capacity(2);
        if let Some(bytes) = load(files) {
            fonts
                .font_data
                .insert(family.to_owned(), Arc::new(FontData::from_owned(bytes)));
            chain.push(family.to_owned());
        }
        chain.push("body".to_owned()); // also covers glyphs the face lacks
        fonts
            .families
            .insert(FontFamily::Name(family.into()), chain);
    }

    ctx.set_fonts(fonts);
    ctx.style_mut(|style| {
        style.interaction.selectable_labels = false;
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::TRANSPARENT;
        style.visuals.window_fill = Color32::TRANSPARENT;
    });
    Ok(())
}

// ------------------------------------------------------------------- public

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    NextShip,
    Close,
    DontShowToday,
}

pub struct ShipView<'a> {
    pub ship: &'a Ship,
    /// `None` when the photo is missing or could not be decoded.
    pub texture: Option<&'a TextureHandle>,
    pub seconds_left: u64,
    /// Opacity of the photo and text (used to fade in a newly picked ship).
    pub content_opacity: f32,
    /// Whether "Next Ship" can lead anywhere.
    pub has_other_ships: bool,
}

/// Paints the ship card and returns what the user asked for this frame.
pub fn show_ship(ctx: &Context, window_opacity: f32, view: &ShipView<'_>) -> Action {
    let mut action = Action::None;
    CentralPanel::default().frame(Frame::new()).show(ctx, |ui| {
        ui.set_opacity(window_opacity);
        let card = card_rect(ui);
        paint_card_background(ui, card);

        ui.scope_builder(UiBuilder::new().max_rect(card), |ui| {
            ui.multiply_opacity(view.content_opacity);
            // The card has a fixed size, so the text is measured first and
            // the photo gives up height if an unusually long text needs it.
            let text_height = measure_body(ui, card, view.ship);
            let max_photo_height = CARD_H
                - BODY_TOP_GAP
                - text_height
                - BODY_BOTTOM_GAP
                - BUTTON_H
                - FOOTER_BOTTOM_PAD;
            let photo = paint_photo(ui, card, view, max_photo_height);
            paint_body(ui, card, photo.bottom(), view.ship);
        });

        pill(
            ui,
            card.left_top() + vec2(14.0, 14.0),
            Align::Min,
            "Daily Ships",
            GOLD_BRIGHT,
        );
        pill(
            ui,
            card.right_top() + vec2(-14.0, 14.0),
            Align::Max,
            &format!("Closes in {} s", view.seconds_left),
            TEXT,
        );

        action = footer(ui, card, view.has_other_ships, true);
        paint_card_border(ui, card);
    });
    action
}

/// Paints a card explaining that nothing can be shown (for example when the
/// ship database is invalid). Only offers "Close".
pub fn show_error(ctx: &Context, window_opacity: f32, message: &str, seconds_left: u64) -> Action {
    let mut action = Action::None;
    CentralPanel::default().frame(Frame::new()).show(ctx, |ui| {
        ui.set_opacity(window_opacity);
        let card = card_rect(ui);
        paint_card_background(ui, card);

        let photo = Rect::from_min_size(card.min, vec2(CARD_W, PHOTO_MIN_H));
        paint_placeholder(ui, photo, "No ship to show");

        let body_rect = Rect::from_min_max(
            pos2(card.left() + PAD, photo.bottom() + 18.0),
            pos2(card.right() - PAD, card.bottom() - 70.0),
        );
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(body_rect)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.add(Label::new(caps("Something went wrong", GOLD)));
                ui.add_space(6.0);
                ui.add(Label::new(
                    RichText::new("Daily Ships").font(serif(27.0)).color(CREAM),
                ));
                ui.add_space(14.0);
                ui.add(
                    Label::new(
                        RichText::new(message)
                            .font(body(13.0))
                            .color(TEXT)
                            .line_height(Some(19.0)),
                    )
                    .wrap(),
                );
            },
        );

        pill(
            ui,
            card.right_top() + vec2(-14.0, 14.0),
            Align::Max,
            &format!("Closes in {seconds_left} s"),
            TEXT,
        );
        action = footer(ui, card, false, false);
        paint_card_border(ui, card);
    });
    action
}

// ------------------------------------------------------------------ pieces

fn card_rect(ui: &Ui) -> Rect {
    Rect::from_min_size(
        ui.max_rect().min + vec2(SHADOW_MARGIN, SHADOW_MARGIN),
        vec2(CARD_W, CARD_H),
    )
}

fn paint_card_background(ui: &Ui, card: Rect) {
    let radius = CornerRadius::same(RADIUS);
    let shadow = Shadow {
        offset: [0, 3],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    ui.painter().add(shadow.as_shape(card, radius));
    ui.painter().rect_filled(card, radius, NAVY);
}

fn paint_card_border(ui: &Ui, card: Rect) {
    ui.painter().rect_stroke(
        card,
        CornerRadius::same(RADIUS),
        Stroke::new(1.0_f32, GOLD.gamma_multiply(0.38)),
        StrokeKind::Inside,
    );
}

/// Paints the photo across the top of the card and returns the area used.
/// The area's height follows the photo's aspect ratio within fixed limits;
/// whatever does not fit is cropped around the ship's focus point.
fn paint_photo(ui: &Ui, card: Rect, view: &ShipView<'_>, max_height: f32) -> Rect {
    let top_corners = CornerRadius {
        nw: RADIUS,
        ne: RADIUS,
        sw: 0,
        se: 0,
    };
    let rect = match view.texture {
        Some(texture) => {
            let aspect = texture.aspect_ratio().max(0.1);
            let height = (CARD_W / aspect)
                .clamp(PHOTO_MIN_H, PHOTO_MAX_H)
                .min(max_height)
                .max(PHOTO_FLOOR_H)
                .round();
            let rect = Rect::from_min_size(card.min, vec2(CARD_W, height));
            Image::from_texture(SizedTexture::from_handle(texture))
                .uv(cover_uv(aspect, CARD_W / height, view.ship.focus))
                .corner_radius(top_corners)
                .paint_at(ui, rect);
            rect
        }
        None => {
            let height = PHOTO_MIN_H.min(max_height).max(PHOTO_FLOOR_H);
            let rect = Rect::from_min_size(card.min, vec2(CARD_W, height));
            paint_placeholder(ui, rect, "Photograph unavailable");
            rect
        }
    };

    // Fade the bottom of the photo into the card.
    let fade = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 72.0), rect.max);
    vertical_gradient(ui, fade, Color32::TRANSPARENT, NAVY);

    if view.texture.is_some() && !view.ship.image_credit.is_empty() {
        let credit = ui.painter().layout_no_wrap(
            view.ship.image_credit.clone(),
            body(9.5),
            Color32::PLACEHOLDER,
        );
        let pos = pos2(
            rect.right() - 14.0 - credit.size().x,
            rect.bottom() - 6.0 - credit.size().y,
        );
        ui.painter()
            .galley(pos, credit, Color32::from_white_alpha(120));
    }
    rect
}

/// The part of a texture (as UV coordinates) that fills an area of the given
/// aspect ratio without distortion, centred on `focus` as far as possible.
fn cover_uv(texture_aspect: f32, area_aspect: f32, focus: [f32; 2]) -> Rect {
    let (width, height) = if texture_aspect > area_aspect {
        (area_aspect / texture_aspect, 1.0)
    } else {
        (1.0, texture_aspect / area_aspect)
    };
    let left = (focus[0] - width / 2.0).clamp(0.0, 1.0 - width);
    let top = (focus[1] - height / 2.0).clamp(0.0, 1.0 - height);
    Rect::from_min_size(pos2(left, top), vec2(width, height))
}

fn paint_placeholder(ui: &Ui, rect: Rect, caption: &str) {
    let top_corners = CornerRadius {
        nw: RADIUS,
        ne: RADIUS,
        sw: 0,
        se: 0,
    };
    ui.painter().rect_filled(rect, top_corners, NAVY_RAISED);

    // A simple horizon with a hull silhouette.
    let centre = rect.center() - vec2(0.0, 14.0);
    let stroke = Stroke::new(1.5_f32, GOLD_DIM);
    let hull = vec![
        centre + vec2(-46.0, 0.0),
        centre + vec2(46.0, 0.0),
        centre + vec2(34.0, 16.0),
        centre + vec2(-34.0, 16.0),
    ];
    ui.painter().add(Shape::closed_line(hull, stroke));
    ui.painter()
        .line_segment([centre + vec2(0.0, 0.0), centre + vec2(0.0, -34.0)], stroke);
    ui.painter().add(Shape::closed_line(
        vec![
            centre + vec2(4.0, -32.0),
            centre + vec2(30.0, -6.0),
            centre + vec2(4.0, -6.0),
        ],
        stroke,
    ));
    ui.painter().line_segment(
        [centre + vec2(-90.0, 26.0), centre + vec2(90.0, 26.0)],
        Stroke::new(1.0_f32, GOLD_DIM.gamma_multiply(0.5)),
    );

    let text = caps_galley(ui, caption, MUTED);
    let pos = pos2(rect.center().x - text.size().x / 2.0, centre.y + 40.0);
    ui.painter().galley(pos, text, MUTED);
}

/// Lays out the text block invisibly and returns the height it needs.
fn measure_body(ui: &mut Ui, card: Rect, ship: &Ship) -> f32 {
    let rect = Rect::from_min_max(
        pos2(card.left() + PAD, card.top()),
        pos2(card.right() - PAD, card.bottom()),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min))
            .invisible(),
        |ui| body_contents(ui, ship),
    )
    .response
    .rect
    .height()
}

fn paint_body(ui: &mut Ui, card: Rect, top: f32, ship: &Ship) {
    let body_rect = Rect::from_min_max(
        pos2(card.left() + PAD, top + BODY_TOP_GAP),
        pos2(
            card.right() - PAD,
            card.bottom() - FOOTER_BOTTOM_PAD - BUTTON_H - 6.0,
        ),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(body_rect)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            // Last line of defence for texts far beyond the documented limits.
            ui.set_clip_rect(body_rect.expand2(vec2(2.0, 0.0)));
            body_contents(ui, ship);
        },
    );
}

fn body_contents(ui: &mut Ui, ship: &Ship) {
    ui.spacing_mut().item_spacing = Vec2::ZERO;

    ui.add(Label::new(caps(&ship.nation, GOLD)));
    ui.add_space(5.0);
    ui.add(Label::new(RichText::new(&ship.name).font(serif(27.0)).color(CREAM)).wrap());
    ui.add_space(10.0);

    let (rule, _) = ui.allocate_exact_size(vec2(36.0, 2.0), Sense::hover());
    ui.painter().rect_filled(rule, 1.0, GOLD);
    ui.add_space(13.0);

    fact_row(ui, "Class", &ship.class);
    fact_row(ui, "Launched", &ship.launched);
    fact_row(ui, "Fate", &ship.fate);
    ui.add_space(11.0);

    ui.add(
        Label::new(
            RichText::new(&ship.description)
                .font(body(13.0))
                .color(TEXT)
                .line_height(Some(19.0)),
        )
        .wrap(),
    );
    ui.add_space(14.0);

    let block = ui
        .horizontal_top(|ui| {
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.add(Label::new(caps("Did you know", GOLD)));
                ui.add_space(4.0);
                ui.add(
                    Label::new(
                        RichText::new(&ship.fun_fact)
                            .font(serif_italic(13.5))
                            .color(CREAM.gamma_multiply(0.92))
                            .line_height(Some(19.0)),
                    )
                    .wrap(),
                );
            });
        })
        .response
        .rect;
    ui.painter().rect_filled(
        Rect::from_min_max(block.left_top(), pos2(block.left() + 2.0, block.bottom())),
        1.0,
        GOLD,
    );
}

fn fact_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal_top(|ui| {
        let (label_rect, _) = ui.allocate_exact_size(vec2(FACT_LABEL_W, 20.0), Sense::hover());
        let galley = caps_galley(ui, label, GOLD_DIM);
        ui.painter()
            .galley(label_rect.min + vec2(0.0, 4.0), galley, GOLD_DIM);
        ui.add(
            Label::new(
                RichText::new(value)
                    .font(body(13.0))
                    .color(CREAM.gamma_multiply(0.95))
                    .line_height(Some(20.0)),
            )
            .wrap(),
        );
    });
}

fn footer(ui: &mut Ui, card: Rect, show_next: bool, show_snooze: bool) -> Action {
    let bottom = card.bottom() - FOOTER_BOTTOM_PAD;
    let rect = Rect::from_min_max(
        pos2(card.left() + PAD, bottom - BUTTON_H),
        pos2(card.right() - PAD, bottom),
    );
    let mut action = Action::None;
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            if show_next && button(ui, "Next Ship", ButtonKind::Primary).clicked() {
                action = Action::NextShip;
            }
            if button(ui, "Close", ButtonKind::Secondary).clicked() {
                action = Action::Close;
            }
            if show_snooze {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, "Don't show today", ButtonKind::Quiet).clicked() {
                        action = Action::DontShowToday;
                    }
                });
            }
        },
    );
    action
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonKind {
    /// Filled gold.
    Primary,
    /// Outlined.
    Secondary,
    /// Text only.
    Quiet,
}

fn button(ui: &mut Ui, label: &str, kind: ButtonKind) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), semibold(12.5), Color32::PLACEHOLDER);
    let pad_x = if kind == ButtonKind::Quiet { 2.0 } else { 17.0 };
    let (rect, response) = ui.allocate_exact_size(
        vec2(galley.size().x + 2.0 * pad_x, BUTTON_H),
        Sense::click(),
    );
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let hovered = response.hovered();
    let pressed = response.is_pointer_button_down_on();

    let radius = CornerRadius::same(8);
    let text_color = match kind {
        ButtonKind::Primary => {
            let fill = if pressed {
                GOLD_DIM
            } else if hovered {
                GOLD_BRIGHT
            } else {
                GOLD
            };
            ui.painter().rect_filled(rect, radius, fill);
            NAVY
        }
        ButtonKind::Secondary => {
            let fill = Color32::from_white_alpha(if pressed {
                28
            } else if hovered {
                16
            } else {
                0
            });
            ui.painter().rect_filled(rect, radius, fill);
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(
                    1.0_f32,
                    Color32::from_white_alpha(if hovered { 70 } else { 42 }),
                ),
                StrokeKind::Inside,
            );
            CREAM
        }
        ButtonKind::Quiet => {
            if hovered {
                let y = rect.center().y + galley.size().y / 2.0 + 1.0;
                ui.painter().line_segment(
                    [pos2(rect.left() + pad_x, y), pos2(rect.right() - pad_x, y)],
                    Stroke::new(1.0_f32, GOLD_DIM),
                );
                CREAM
            } else {
                MUTED
            }
        }
    };

    let text_pos = rect.center() - galley.size() / 2.0;
    ui.painter().galley(text_pos, galley, text_color);
    response
}

/// Small label on a dark translucent capsule, drawn over the photo.
/// `anchor` is the capsule's top-left (`Align::Min`) or top-right corner.
fn pill(ui: &Ui, anchor: Pos2, align: Align, text: &str, color: Color32) {
    let galley = caps_galley(ui, text, color);
    let size = galley.size() + vec2(20.0, 10.0);
    let left = if align == Align::Max {
        anchor.x - size.x
    } else {
        anchor.x
    };
    let rect = Rect::from_min_size(pos2(left, anchor.y), size);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(11),
        Color32::from_rgba_unmultiplied(6, 12, 24, 190),
    );
    ui.painter()
        .galley(rect.min + vec2(10.0, 5.0), galley, color);
}

/// Small, letter-spaced capitals used for eyebrows and labels.
fn caps(text: &str, color: Color32) -> RichText {
    RichText::new(text.to_uppercase())
        .font(semibold(9.5))
        .color(color)
        .extra_letter_spacing(1.3)
}

fn caps_galley(ui: &Ui, text: &str, color: Color32) -> Arc<Galley> {
    WidgetText::from(caps(text, color)).into_galley(
        ui,
        Some(TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Body,
    )
}

fn vertical_gradient(ui: &Ui, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_uv_stays_inside_the_texture_and_keeps_aspect() {
        for (texture_aspect, area_aspect) in [(1.3, 1.78), (2.4, 1.78), (1.78, 1.78), (0.7, 2.2)] {
            for focus in [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0], [0.2, 0.9]] {
                let uv = cover_uv(texture_aspect, area_aspect, focus);
                assert!(uv.min.x >= -1e-6 && uv.min.y >= -1e-6, "{uv:?}");
                assert!(uv.max.x <= 1.0 + 1e-6 && uv.max.y <= 1.0 + 1e-6, "{uv:?}");
                let shown_aspect = uv.width() * texture_aspect / uv.height();
                assert!((shown_aspect - area_aspect).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn corner_position_hugs_the_bottom_right_of_the_work_area() {
        let area = WorkArea {
            left: 0.0,
            top: 0.0,
            right: 1920.0,
            bottom: 1032.0,
        };
        let pos = corner_position(area, 1.0);
        assert_eq!(pos.x + window_size().x, 1920.0);
        assert_eq!(pos.y + window_size().y, 1032.0);

        let scaled = corner_position(
            WorkArea {
                right: 2880.0,
                bottom: 1548.0,
                ..area
            },
            1.5,
        );
        assert_eq!(scaled, pos);
    }
}
