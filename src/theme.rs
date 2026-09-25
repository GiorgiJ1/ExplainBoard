// A small, fixed color palette so the whole app looks consistent instead of
// scattering random colors through the renderer and UI code.

use eframe::egui::{Color32, Frame, Margin};

// Dark application "chrome" — the top bar and toolbar.
pub const APP_BACKGROUND: Color32 = Color32::from_rgb(24, 24, 27);
pub const TEXT_ON_DARK: Color32 = Color32::from_rgb(235, 235, 240);

// The whiteboard itself.
pub const CANVAS_BACKGROUND: Color32 = Color32::from_rgb(250, 250, 252);
pub const GRID_DOT: Color32 = Color32::from_rgb(222, 222, 228);

// Diagram elements (boxes/circles).
pub const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
pub const BORDER: Color32 = Color32::from_rgb(200, 200, 208);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(120, 120, 135);

// Text.
pub const TEXT: Color32 = Color32::from_rgb(30, 30, 35);
pub const MUTED_TEXT: Color32 = Color32::from_rgb(140, 140, 150);

// Accent + status colors.
pub const ACCENT: Color32 = Color32::from_rgb(99, 102, 241); // indigo
pub const SELECTED_BORDER: Color32 = ACCENT;
pub const OK_GREEN: Color32 = Color32::from_rgb(52, 199, 89);
pub const ERROR_RED: Color32 = Color32::from_rgb(230, 80, 80);

/// The dark frame used for the top bar and the bottom toolbar.
pub fn dark_frame() -> Frame {
    Frame::none()
        .fill(APP_BACKGROUND)
        .inner_margin(Margin::same(12.0))
}