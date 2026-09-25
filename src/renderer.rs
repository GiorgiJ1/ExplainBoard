// This file is only about drawing. It knows nothing about Ollama and
// nothing about mouse dragging — main.rs handles those and just calls
// draw_diagram() once per frame.

use crate::diagram::{Diagram, Element};
use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

/// Tracks how far the whiteboard has been panned and how zoomed in it is.
///
/// "World" coordinates are the coordinates inside the diagram JSON (e.g. a
/// box at x=100, y=100 — these never change just because you scroll or zoom).
/// "Screen" coordinates are actual pixel positions in the window. This
/// struct is the only place that converts between the two.
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
}

/// Draws every element in the diagram onto the canvas. Arrows are drawn
/// first so boxes and circles visually sit on top of the lines connecting
/// them, instead of lines being drawn over the shapes.
pub fn draw_diagram(painter: &Painter, canvas_origin: Pos2, diagram: &Diagram, camera: &Camera) {
    for element in &diagram.elements {
        if let Element::Arrow { from, to, text } = element {
            let from_point = find_center(diagram, from);
            let to_point = find_center(diagram, to);
            if let (Some(from_world), Some(to_world)) = (from_point, to_point) {
                let start = camera.world_to_screen(canvas_origin, Pos2::new(from_world.0, from_world.1));
                let end = camera.world_to_screen(canvas_origin, Pos2::new(to_world.0, to_world.1));
                draw_arrow(painter, start, end, text);
            }
            // If either id doesn't exist in the diagram, we just skip this
            // arrow instead of crashing. main.rs checks for this too and
            // reports it in the status bar before rendering.
        }
    }

    for element in &diagram.elements {
        let font_size = 14.0 * camera.zoom.max(0.4);
        match element {
            Element::Box { x, y, width, height, text, .. } => {
                let top_left = camera.world_to_screen(canvas_origin, Pos2::new(*x, *y));
                let size = Vec2::new(*width, *height) * camera.zoom;
                let rect = Rect::from_min_size(top_left, size);
                painter.rect_filled(rect, 4.0, Color32::from_rgb(90, 140, 220));
                painter.rect_stroke(rect, 4.0, Stroke::new(2.0, Color32::BLACK), StrokeKind::Outside);
                painter.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(font_size), Color32::WHITE);
            }
            Element::Circle { x, y, radius, text, .. } => {
                let center = camera.world_to_screen(canvas_origin, Pos2::new(*x, *y));
                let screen_radius = radius * camera.zoom;
                painter.circle_filled(center, screen_radius, Color32::from_rgb(220, 140, 90));
                painter.circle_stroke(center, screen_radius, Stroke::new(2.0, Color32::BLACK));
                painter.text(center, Align2::CENTER_CENTER, text, FontId::proportional(font_size), Color32::WHITE);
            }
            Element::Text { x, y, text, .. } => {
                let pos = camera.world_to_screen(canvas_origin, Pos2::new(*x, *y));
                painter.text(pos, Align2::LEFT_TOP, text, FontId::proportional(font_size), Color32::BLACK);
            }
            Element::Arrow { .. } => {} // already drawn in the loop above
        }
    }
}

fn find_center(diagram: &Diagram, id: &str) -> Option<(f32, f32)> {
    diagram.elements.iter().find(|element| element.id() == Some(id)).and_then(|element| element.center())
}

fn draw_arrow(painter: &Painter, start: Pos2, end: Pos2, label: &str) {
    painter.line_segment([start, end], Stroke::new(2.0, Color32::DARK_GRAY));

    // A simple arrowhead: two short lines angled back from the tip.
    let direction = (end - start).normalized();
    let head_length = 10.0;
    let head_angle = 0.4_f32; // radians
    let left_wing = rotate(direction, head_angle) * -head_length;
    let right_wing = rotate(direction, -head_angle) * -head_length;
    painter.line_segment([end, end + left_wing], Stroke::new(2.0, Color32::DARK_GRAY));
    painter.line_segment([end, end + right_wing], Stroke::new(2.0, Color32::DARK_GRAY));

    if !label.is_empty() {
        let midpoint = start + (end - start) * 0.5;
        painter.text(
            midpoint - Vec2::new(0.0, 12.0),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.0),
            Color32::DARK_GRAY,
        );
    }
}

fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(vector.x * cos - vector.y * sin, vector.x * sin + vector.y * cos)
}