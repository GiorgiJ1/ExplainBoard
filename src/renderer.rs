// This file is only about drawing. It knows nothing about Ollama and
// nothing about mouse dragging — main.rs handles input and just calls the
// functions here once per frame.

use crate::layout::{LaidOutDiagram, LayoutArrow};
use crate::theme;
use eframe::egui::{Align2, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

/// Tracks how far the whiteboard has been panned and how zoomed in it is.
///
/// "World" coordinates are the coordinates the layout engine assigns (e.g. a
/// box at x=100, y=100 — these never change just because you scroll or
/// zoom). "Screen" coordinates are actual pixel positions in the window.
/// This struct is the only place that converts between the two.
pub struct Camera {
    pub pan: Vec2,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { pan: Vec2::ZERO, zoom: 1.0 }
    }
}

impl Camera {
    pub fn world_to_screen(&self, canvas_origin: Pos2, world: Pos2) -> Pos2 {
        canvas_origin + self.pan + world.to_vec2() * self.zoom
    }

    pub fn screen_to_world(&self, canvas_origin: Pos2, screen: Pos2) -> Pos2 {
        (((screen - canvas_origin) - self.pan) / self.zoom).to_pos2()
    }

    /// Sets pan and zoom so that `bounds` (world space) is fully visible and
    /// centered within a canvas of size `canvas_size`, with a comfortable margin.
    pub fn fit(&mut self, canvas_size: Vec2, bounds: Rect) {
        let margin = 60.0;
        let available = Vec2::new((canvas_size.x - margin * 2.0).max(50.0), (canvas_size.y - margin * 2.0).max(50.0));
        let bounds_size = bounds.size().max(Vec2::new(1.0, 1.0));
        let zoom_x = available.x / bounds_size.x;
        let zoom_y = available.y / bounds_size.y;
        self.zoom = zoom_x.min(zoom_y).clamp(0.1, 2.5);
        self.pan = canvas_size / 2.0 - bounds.center().to_vec2() * self.zoom;
    }
}

/// Draws a subtle dot grid across the canvas so the whiteboard feels
/// intentional instead of an empty rectangle. Skipped when zoomed out far
/// enough that the dots would merge into a gray smear.
pub fn draw_grid(painter: &Painter, canvas_rect: Rect, camera: &Camera) {
    let spacing_world = 40.0;
    let spacing_screen = spacing_world * camera.zoom;
    if spacing_screen < 8.0 {
        return;
    }

    let origin_screen = camera.world_to_screen(canvas_rect.min, Pos2::ZERO);
    let start_x = canvas_rect.min.x - (canvas_rect.min.x - origin_screen.x).rem_euclid(spacing_screen);
    let start_y = canvas_rect.min.y - (canvas_rect.min.y - origin_screen.y).rem_euclid(spacing_screen);

    let mut y = start_y;
    while y < canvas_rect.max.y {
        let mut x = start_x;
        while x < canvas_rect.max.x {
            painter.circle_filled(Pos2::new(x, y), 1.2, theme::GRID_DOT);
            x += spacing_screen;
        }
        y += spacing_screen;
    }
}

/// Draws the diagram's title above the diagram, in world space, so it pans
/// and zooms along with the rest of the content.
pub fn draw_title(painter: &Painter, canvas_origin: Pos2, layout: &LaidOutDiagram, camera: &Camera) {
    if layout.title.is_empty() {
        return;
    }
    let bounds = layout.bounds();
    let title_world = Pos2::new(bounds.center().x, bounds.min.y + 30.0);
    let pos = camera.world_to_screen(canvas_origin, title_world);
    let font_size = (20.0 * camera.zoom).max(8.0);
    painter.text(pos, Align2::CENTER_CENTER, &layout.title, FontId::proportional(font_size), theme::TEXT);
}

/// Draws every box, circle, and arrow. `arrows` is passed in separately
/// (rather than read from `layout`) because main.rs combines the dynamic,
/// id-based arrows with the fixed ping-pong arrows before calling this.
pub fn draw_diagram(
    painter: &Painter,
    canvas_origin: Pos2,
    layout: &LaidOutDiagram,
    arrows: &[LayoutArrow],
    camera: &Camera,
    selected_id: Option<&str>,
) {
    // Arrows first, so boxes/circles visually sit on top of the lines
    // connecting them.
    for arrow in arrows {
        let start = camera.world_to_screen(canvas_origin, arrow.start);
        let end = camera.world_to_screen(canvas_origin, arrow.end);
        draw_arrow(painter, start, end, &arrow.text, camera.zoom);
    }

    for b in &layout.boxes {
        let top_left = camera.world_to_screen(canvas_origin, b.rect.min);
        let size = b.rect.size() * camera.zoom;
        let rect = Rect::from_min_size(top_left, size);
        let selected = selected_id == Some(b.id.as_str());
        draw_box_shape(painter, rect, selected);
        draw_centered_lines(painter, rect.center(), &b.lines, camera.zoom);
    }

    for c in &layout.circles {
        let center = camera.world_to_screen(canvas_origin, c.center);
        let radius = c.radius * camera.zoom;
        let selected = selected_id == Some(c.id.as_str());
        draw_circle_shape(painter, center, radius, selected);
        draw_centered_lines(painter, center, &c.lines, camera.zoom);
    }
}

fn draw_box_shape(painter: &Painter, rect: Rect, selected: bool) {
    let border_color = if selected { theme::SELECTED_BORDER } else { theme::BORDER_STRONG };
    let border_width = if selected { 2.5_f32 } else { 1.5_f32 };
    painter.rect_filled(rect, 8.0, theme::SURFACE);
    painter.rect_stroke(rect, 8.0, Stroke::new(border_width, border_color), StrokeKind::Outside);
}

fn draw_circle_shape(painter: &Painter, center: Pos2, radius: f32, selected: bool) {
    let border_color = if selected { theme::SELECTED_BORDER } else { theme::BORDER_STRONG };
    let border_width = if selected { 2.5_f32 } else { 1.5_f32 };
    painter.circle_filled(center, radius, theme::SURFACE);
    painter.circle_stroke(center, radius, Stroke::new(border_width, border_color));
}

fn draw_centered_lines(painter: &Painter, center: Pos2, lines: &[String], zoom: f32) {
    let font_size = (14.0 * zoom).max(6.0);
    let line_height = font_size * 1.4;
    let total_height = line_height * lines.len() as f32;
    let mut y = center.y - total_height / 2.0 + line_height / 2.0;
    for line in lines {
        painter.text(Pos2::new(center.x, y), Align2::CENTER_CENTER, line, FontId::proportional(font_size), theme::TEXT);
        y += line_height;
    }
}

fn draw_arrow(painter: &Painter, start: Pos2, end: Pos2, label: &str, zoom: f32) {
    painter.line_segment([start, end], Stroke::new(1.8_f32, theme::BORDER_STRONG));

    // A simple arrowhead: two short lines angled back from the tip.
    let direction = (end - start).normalized();
    let head_length = 9.0_f32;
    let head_angle = 0.45_f32; // radians
    let left_wing = rotate(direction, head_angle) * -head_length;
    let right_wing = rotate(direction, -head_angle) * -head_length;
    painter.line_segment([end, end + left_wing], Stroke::new(1.8_f32, theme::BORDER_STRONG));
    painter.line_segment([end, end + right_wing], Stroke::new(1.8_f32, theme::BORDER_STRONG));

    if !label.is_empty() {
        let midpoint = start + (end - start) * 0.5;
        let font_size = (13.0 * zoom).max(6.0);
        // A small background behind the label so it stays readable over the
        // grid or the line itself, instead of floating awkwardly.
        let galley = painter.layout_no_wrap(label.to_string(), FontId::proportional(font_size), theme::MUTED_TEXT);
        let label_pos = midpoint - Vec2::new(0.0, 14.0);
        let label_rect = Rect::from_center_size(label_pos, galley.size() + Vec2::new(8.0, 4.0));
        painter.rect_filled(label_rect, 4.0, theme::CANVAS_BACKGROUND);
        painter.text(label_rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(font_size), theme::TEXT);
    }
}

fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(vector.x * cos - vector.y * sin, vector.x * sin + vector.y * cos)
}