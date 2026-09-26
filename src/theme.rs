// A small, fixed color palette so the whole app looks consistent instead of
// scattering random colors through the renderer and UI code. This is the
// "dark neon glass" theme: near-black surfaces, a glowing teal accent.

use eframe::egui::{Color32, Frame, Margin};

// Dark application "chrome" — the top bar, left toolbar, bottom AI bar.
pub const APP_BACKGROUND: Color32 = Color32::from_rgb(10, 13, 16);
pub const TEXT_ON_DARK: Color32 = Color32::from_rgb(230, 240, 240);

// The whiteboard itself — near-black, so glowing elements read clearly.
pub const CANVAS_BACKGROUND: Color32 = Color32::from_rgb(7, 10, 12);
pub const GRID_DOT: Color32 = Color32::from_rgb(28, 48, 48);

// Diagram elements: glassy dark cards with a glowing accent border.
pub const SURFACE: Color32 = Color32::from_rgb(18, 24, 28);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(70, 200, 190);

// Text.
pub const TEXT: Color32 = Color32::from_rgb(225, 245, 245);
pub const MUTED_TEXT: Color32 = Color32::from_rgb(120, 150, 150);

// The signature glowing teal/cyan accent, and a brighter variant used to
// make the selected element clearly stand out from the rest.
pub const ACCENT: Color32 = Color32::from_rgb(45, 212, 191);
pub const SELECTED_BORDER: Color32 = Color32::from_rgb(110, 245, 225);

pub const OK_GREEN: Color32 = Color32::from_rgb(52, 211, 153);
pub const ERROR_RED: Color32 = Color32::from_rgb(248, 113, 113);

/// The dark frame used for the top bar, left toolbar, and bottom AI bar.
pub fn dark_frame() -> Frame {
    Frame::NONE
        .fill(APP_BACKGROUND)
        .inner_margin(Margin::same(12))
}