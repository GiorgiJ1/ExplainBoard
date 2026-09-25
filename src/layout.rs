// This is where positions come from. Ollama never sends coordinates — it
// only sends element types, ids, text, and arrows between ids. This file
// turns that into actual world-space rectangles, circles, and connecting
// lines, deterministically, with no AI involved.
//
// Two layout patterns are supported:
//   - vertical flow: nodes stacked top to bottom, in the order they appear.
//     Used for anything that isn't the pattern below (DNS chain, physics
//     cause-and-effect, etc).
//   - two-column "ping-pong": exactly two nodes with several arrows going
//     back and forth between them (a request/response handshake). Drawn as
//     two side-by-side boxes with one arrow row per message, like a
//     simplified sequence diagram.
//
// Positions produced here are NOT final and frozen: dragging an element
// later (in main.rs) mutates the box/circle positions stored in
// `LaidOutDiagram` directly, and `resolve_arrows()` recomputes arrow
// endpoints live from wherever the boxes/circles currently are.

use crate::diagram::{Diagram, Element};
use eframe::egui::{Pos2, Rect, Vec2};

const BOX_MIN_WIDTH: f32 = 140.0;
const BOX_MAX_WIDTH: f32 = 260.0;
const BOX_PADDING_X: f32 = 18.0;
const BOX_PADDING_Y: f32 = 14.0;
const LINE_HEIGHT: f32 = 20.0;
const CHAR_WIDTH_ESTIMATE: f32 = 7.5; // rough average glyph width at 14px
const CIRCLE_MIN_RADIUS: f32 = 45.0;

const VERTICAL_GAP: f32 = 90.0; // space between stacked nodes (room for an arrow + label)
const COLUMN_GAP: f32 = 260.0; // horizontal space between the two columns in ping-pong layout
const PINGPONG_ROW_GAP: f32 = 56.0; // vertical space between successive ping-pong arrows

pub struct LayoutBox {
    pub id: String,
    pub lines: Vec<String>, // pre-wrapped text, ready to draw line by line
    pub rect: Rect,         // world-space; dragging mutates this directly
}

pub struct LayoutCircle {
    pub id: String,
    pub lines: Vec<String>,
    pub center: Pos2, // world-space; dragging mutates this directly
    pub radius: f32,
}

/// An arrow that only knows the *ids* it connects. Its actual start/end
/// points are computed on demand by `resolve_arrows()`, so if a box moves,
/// the arrow follows automatically.
pub struct ArrowLink {
    pub from: String,
    pub to: String,
    pub text: String,
}

/// An arrow with concrete world-space start/end points, ready to draw.
#[derive(Clone)]
pub struct LayoutArrow {
    pub start: Pos2,
    pub end: Pos2,
    pub text: String,
}

pub struct LaidOutDiagram {
    pub title: String,
    pub boxes: Vec<LayoutBox>,
    pub circles: Vec<LayoutCircle>,
    /// Dynamic arrows: recomputed live from current box/circle positions.
    /// Used by the vertical-flow layout.
    pub arrow_links: Vec<ArrowLink>,
    /// Static arrows: fixed at layout time. Used by the two-column ping-pong
    /// layout, whose arrow rows are meaningful positions in their own right,
    /// not just "shortest line between two centers".
    pub fixed_arrows: Vec<LayoutArrow>,
}

#[derive(Clone, Copy)]
enum NodeShape {
    Box { half_width: f32, half_height: f32 },
    Circle { radius: f32 },
}

impl LaidOutDiagram {
    /// The current bounding box of every element, used to auto-fit the
    /// camera. Always computed fresh from wherever elements currently are
    /// (including after dragging), never cached.
    pub fn bounds(&self) -> Rect {
        let mut bounds = Rect::NOTHING;
        for b in &self.boxes {
            bounds = bounds.union(b.rect);
        }
        for c in &self.circles {
            bounds = bounds.union(Rect::from_center_size(c.center, Vec2::splat(c.radius * 2.0)));
        }
        if !bounds.is_finite() {
            bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
        }
        if !self.title.is_empty() {
            // Reserve room above the diagram for the title.
            bounds.min.y -= 70.0;
        }
        bounds
    }

    fn find_shape(&self, id: &str) -> Option<(Pos2, NodeShape)> {
        if let Some(b) = self.boxes.iter().find(|b| b.id == id) {
            let half = b.rect.size() / 2.0;
            return Some((b.rect.center(), NodeShape::Box { half_width: half.x, half_height: half.y }));
        }
        if let Some(c) = self.circles.iter().find(|c| c.id == id) {
            return Some((c.center, NodeShape::Circle { radius: c.radius }));
        }
        None
    }

    pub fn shape_center(&self, id: &str) -> Option<Pos2> {
        self.find_shape(id).map(|(center, _)| center)
    }

    /// Moves a box or circle so its center is at `new_center`. Does nothing
    /// if the id isn't found.
    pub fn set_shape_center(&mut self, id: &str, new_center: Pos2) {
        if let Some(b) = self.boxes.iter_mut().find(|b| b.id == id) {
            let size = b.rect.size();
            b.rect = Rect::from_center_size(new_center, size);
            return;
        }
        if let Some(c) = self.circles.iter_mut().find(|c| c.id == id) {
            c.center = new_center;
        }
    }

    /// Returns the id of the topmost element under `point` (world space), if any.
    pub fn hit_test(&self, point: Pos2) -> Option<String> {
        for b in &self.boxes {
            if b.rect.contains(point) {
                return Some(b.id.clone());
            }
        }
        for c in &self.circles {
            if (point - c.center).length() <= c.radius {
                return Some(c.id.clone());
            }
        }
        None
    }

    /// Computes concrete start/end points for every dynamic arrow link,
    /// clipped to the edge of the shapes it connects (not their centers).
    /// Combine with `fixed_arrows` to get the full set to draw.
    pub fn resolve_arrows(&self) -> Vec<LayoutArrow> {
        let mut result = Vec::new();
        for link in &self.arrow_links {
            if let (Some((from_center, from_shape)), Some((to_center, to_shape))) =
                (self.find_shape(&link.from), self.find_shape(&link.to))
            {
                let start = clip_to_edge(from_center, to_center, from_shape);
                let end = clip_to_edge(to_center, from_center, to_shape);
                result.push(LayoutArrow { start, end, text: link.text.clone() });
            }
            // A missing id means the arrow is skipped here; Diagram::validate()
            // already reported it as a warning before we got this far.
        }
        result
    }
}

/// Finds the point on the edge of a shape, closest to `toward`, starting
/// from `center`. This is what makes arrows connect edge-to-edge instead of
/// passing through the middle of a box.
fn clip_to_edge(center: Pos2, toward: Pos2, shape: NodeShape) -> Pos2 {
    let direction = toward - center;
    if direction.length() < 0.001 {
        return center;
    }
    match shape {
        NodeShape::Circle { radius } => center + direction.normalized() * radius,
        NodeShape::Box { half_width, half_height } => {
            let scale_x = if direction.x.abs() > 0.001 { half_width / direction.x.abs() } else { f32::INFINITY };
            let scale_y = if direction.y.abs() > 0.001 { half_height / direction.y.abs() } else { f32::INFINITY };
            center + direction * scale_x.min(scale_y)
        }
    }
}

/// Splits `text` into lines that each fit within `max_width`, breaking on
/// word boundaries. Never lets a word overflow the box — long single words
/// are just left on their own line (rare in short diagram labels).
fn wrap_text(text: &str, max_width: f32) -> Vec<String> {
    let max_chars_per_line = ((max_width - BOX_PADDING_X * 2.0) / CHAR_WIDTH_ESTIMATE).floor().max(6.0) as usize;
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if candidate.chars().count() > max_chars_per_line && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Figures out a box's wrapped text, width, and height from its label.
fn size_box(text: &str) -> (Vec<String>, f32, f32) {
    let lines = wrap_text(text, BOX_MAX_WIDTH);
    let longest_line_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
    let content_width = longest_line_chars * CHAR_WIDTH_ESTIMATE + BOX_PADDING_X * 2.0;
    let width = content_width.clamp(BOX_MIN_WIDTH, BOX_MAX_WIDTH);
    let height = lines.len() as f32 * LINE_HEIGHT + BOX_PADDING_Y * 2.0;
    (lines, width, height)
}

/// Figures out a circle's wrapped text and radius from its label.
fn size_circle(text: &str) -> (Vec<String>, f32) {
    let lines = wrap_text(text, CIRCLE_MIN_RADIUS * 2.4);
    let longest_line_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
    let needed_radius = (longest_line_chars * CHAR_WIDTH_ESTIMATE / 2.0 + 14.0).max(CIRCLE_MIN_RADIUS);
    (lines, needed_radius)
}

/// The entry point: picks a layout pattern and lays out the whole diagram.
pub fn layout(diagram: &Diagram) -> LaidOutDiagram {
    let nodes: Vec<&Element> = diagram
        .elements
        .iter()
        .filter(|e| matches!(e, Element::Box { .. } | Element::Circle { .. }))
        .collect();
    let arrows: Vec<&Element> = diagram.elements.iter().filter(|e| matches!(e, Element::Arrow { .. })).collect();

    let distinct_ids: Vec<&str> = nodes.iter().filter_map(|e| e.id()).collect();

    if distinct_ids.len() == 2 && arrows.len() >= 2 {
        layout_two_column(diagram, nodes, arrows, distinct_ids[0], distinct_ids[1])
    } else {
        layout_vertical(diagram, nodes, arrows)
    }
}

fn layout_vertical(diagram: &Diagram, nodes: Vec<&Element>, arrows: Vec<&Element>) -> LaidOutDiagram {
    let mut boxes = Vec::new();
    let mut circles = Vec::new();
    let mut cursor_y = 0.0_f32;

    for element in &nodes {
        match element {
            Element::Box { id, text } => {
                let (lines, width, height) = size_box(text);
                let rect = Rect::from_min_size(Pos2::new(-width / 2.0, cursor_y), Vec2::new(width, height));
                boxes.push(LayoutBox { id: id.clone(), lines, rect });
                cursor_y += height + VERTICAL_GAP;
            }
            Element::Circle { id, text } => {
                let (lines, radius) = size_circle(text);
                let center = Pos2::new(0.0, cursor_y + radius);
                circles.push(LayoutCircle { id: id.clone(), lines, center, radius });
                cursor_y += radius * 2.0 + VERTICAL_GAP;
            }
            Element::Arrow { .. } => {}
        }
    }

    let arrow_links = arrows
        .iter()
        .filter_map(|e| {
            if let Element::Arrow { from, to, text } = e {
                Some(ArrowLink { from: from.clone(), to: to.clone(), text: text.clone() })
            } else {
                None
            }
        })
        .collect();

    LaidOutDiagram { title: diagram.title.clone(), boxes, circles, arrow_links, fixed_arrows: Vec::new() }
}

fn layout_two_column(
    diagram: &Diagram,
    nodes: Vec<&Element>,
    arrows: Vec<&Element>,
    left_id: &str,
    right_id: &str,
) -> LaidOutDiagram {
    let mut boxes = Vec::new();
    let mut circles = Vec::new();

    let left_x = -COLUMN_GAP / 2.0;
    let right_x = COLUMN_GAP / 2.0;

    for element in &nodes {
        let Some(id) = element.id() else { continue };
        let x = if id == left_id {
            left_x
        } else if id == right_id {
            right_x
        } else {
            continue;
        };

        match element {
            Element::Box { text, .. } => {
                let (lines, width, height) = size_box(text);
                let rect = Rect::from_min_size(Pos2::new(x - width / 2.0, 0.0), Vec2::new(width, height));
                boxes.push(LayoutBox { id: id.to_string(), lines, rect });
            }
            Element::Circle { text, .. } => {
                let (lines, radius) = size_circle(text);
                let center = Pos2::new(x, radius);
                circles.push(LayoutCircle { id: id.to_string(), lines, center, radius });
            }
            Element::Arrow { .. } => {}
        }
    }

    let nodes_bottom = boxes
        .iter()
        .map(|b| b.rect.max.y)
        .chain(circles.iter().map(|c| c.center.y + c.radius))
        .fold(0.0_f32, f32::max);

    let mut arrow_y = nodes_bottom + PINGPONG_ROW_GAP;
    let mut fixed_arrows = Vec::new();

    for arrow in &arrows {
        if let Element::Arrow { from, to, text } = arrow {
            let going_right = from == left_id;
            let (start_x, end_x) = if going_right { (left_x, right_x) } else { (right_x, left_x) };
            fixed_arrows.push(LayoutArrow {
                start: Pos2::new(start_x, arrow_y),
                end: Pos2::new(end_x, arrow_y),
                text: text.clone(),
            });
            arrow_y += PINGPONG_ROW_GAP;
        }
    }

    LaidOutDiagram { title: diagram.title.clone(), boxes, circles, arrow_links: Vec::new(), fixed_arrows }
}