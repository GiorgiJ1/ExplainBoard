// This is where positions come from, AND where the board's state actually
// lives once a diagram has been generated. Ollama never sends coordinates —
// it only sends element types/ids/text and arrows between ids (at
// generation time), or structured operations describing a change (at
// modification time, see operations.rs). This file turns either of those
// into actual world-space rectangles, circles, and connecting lines.
//
// IMPORTANT: after the initial `layout()` call, `LaidOutDiagram` IS the
// board. Dragging, deleting, duplicating, editing text, and applying AI
// operations all mutate it directly — nothing ever throws it away and
// recomputes from scratch, which is what lets the user's manual edits and
// the AI's edits coexist instead of the AI silently undoing everything.

use crate::diagram::{Diagram, Element};
use crate::operations::{NewElement, Operation};
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
const DUPLICATE_OFFSET: f32 = 30.0; // how far a duplicated element is nudged from its original
const MOVE_NEAR_OFFSET: f32 = 180.0; // how far "move near X" places an element from X

#[derive(Clone)]
pub struct LayoutBox {
    pub id: String,
    pub lines: Vec<String>, // pre-wrapped text, ready to draw line by line
    pub rect: Rect,         // world-space; dragging/editing mutates this directly
}

#[derive(Clone)]
pub struct LayoutCircle {
    pub id: String,
    pub lines: Vec<String>,
    pub center: Pos2, // world-space; dragging/editing mutates this directly
    pub radius: f32,
}

/// An arrow that only knows the *ids* it connects. Its actual start/end
/// points are computed on demand by `resolve_arrows()`, so if a box moves,
/// the arrow follows automatically.
#[derive(Clone)]
pub struct ArrowLink {
    pub from: String,
    pub to: String,
    pub text: String,
}

/// An arrow with concrete world-space start/end points, ready to draw.
/// `from`/`to` are kept alongside (even though the points are already
/// computed) so the board can still be described and validated by id.
#[derive(Clone)]
pub struct LayoutArrow {
    pub start: Pos2,
    pub end: Pos2,
    pub text: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Clone)]
pub struct LaidOutDiagram {
    pub title: String,
    pub boxes: Vec<LayoutBox>,
    pub circles: Vec<LayoutCircle>,
    /// Dynamic arrows: recomputed live from current box/circle positions.
    /// Used by the vertical-flow layout and by anything added afterward.
    pub arrow_links: Vec<ArrowLink>,
    /// Static arrows: fixed at layout time. Used by the two-column
    /// ping-pong layout, whose arrow rows are meaningful positions in their
    /// own right, not just "shortest line between two centers".
    pub fixed_arrows: Vec<LayoutArrow>,
}

#[derive(Clone, Copy)]
enum NodeShape {
    Box { half_width: f32, half_height: f32 },
    Circle { radius: f32 },
}

impl LaidOutDiagram {
    // ---------------------------------------------------------------
    // Reading the board
    // ---------------------------------------------------------------

    /// The current bounding box of every element, used to auto-fit the
    /// camera and to place newly added elements. Always computed fresh,
    /// never cached, so it's correct even after dragging/edits.
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
            bounds.min.y -= 70.0; // reserve room for the title
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

    pub fn element_exists(&self, id: &str) -> bool {
        self.boxes.iter().any(|b| b.id == id) || self.circles.iter().any(|c| c.id == id)
    }

    pub fn element_text(&self, id: &str) -> Option<String> {
        if let Some(b) = self.boxes.iter().find(|b| b.id == id) {
            return Some(b.lines.join(" "));
        }
        if let Some(c) = self.circles.iter().find(|c| c.id == id) {
            return Some(c.lines.join(" "));
        }
        None
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

    /// A plain-text summary of the board's semantic content (ids, text,
    /// connections) — this, not a screenshot or raw coordinates, is what
    /// gets sent to Ollama so it understands "the current whiteboard".
    pub fn describe(&self) -> String {
        let mut out = String::new();
        if !self.title.is_empty() {
            out.push_str(&format!("Title: {}\n", self.title));
        }
        out.push_str("Elements:\n");
        for b in &self.boxes {
            out.push_str(&format!("- id: {}, text: \"{}\"\n", b.id, b.lines.join(" ")));
        }
        for c in &self.circles {
            out.push_str(&format!("- id: {}, text: \"{}\"\n", c.id, c.lines.join(" ")));
        }
        out.push_str("Arrows:\n");
        for link in &self.arrow_links {
            out.push_str(&format!("- {} -> {} ({})\n", link.from, link.to, link.text));
        }
        for arrow in &self.fixed_arrows {
            if let (Some(from), Some(to)) = (&arrow.from, &arrow.to) {
                out.push_str(&format!("- {} -> {} ({})\n", from, to, arrow.text));
            }
        }
        out
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
                result.push(LayoutArrow {
                    start,
                    end,
                    text: link.text.clone(),
                    from: Some(link.from.clone()),
                    to: Some(link.to.clone()),
                });
            }
            // A missing id means the arrow is skipped here; remove_element()
            // already removes any links that would go dangling, so this is
            // just a safety net.
        }
        result
    }

    // ---------------------------------------------------------------
    // Mutating the board (used by both local UI actions and AI operations)
    // ---------------------------------------------------------------

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

    pub fn add_box(&mut self, id: String, text: String) -> Result<(), String> {
        if self.element_exists(&id) {
            return Err(format!("id \"{id}\" already exists"));
        }
        let (lines, width, height) = size_box(&text);
        let bounds = self.bounds();
        let position = Pos2::new(bounds.center().x - width / 2.0, bounds.max.y + VERTICAL_GAP);
        let rect = Rect::from_min_size(position, Vec2::new(width, height));
        self.boxes.push(LayoutBox { id, lines, rect });
        Ok(())
    }

    pub fn add_circle(&mut self, id: String, text: String) -> Result<(), String> {
        if self.element_exists(&id) {
            return Err(format!("id \"{id}\" already exists"));
        }
        let (lines, radius) = size_circle(&text);
        let bounds = self.bounds();
        let center = Pos2::new(bounds.center().x, bounds.max.y + VERTICAL_GAP + radius);
        self.circles.push(LayoutCircle { id, lines, center, radius });
        Ok(())
    }

    pub fn remove_element(&mut self, id: &str) -> Result<(), String> {
        let before = self.boxes.len() + self.circles.len();
        self.boxes.retain(|b| b.id != id);
        self.circles.retain(|c| c.id != id);
        if self.boxes.len() + self.circles.len() == before {
            return Err(format!("element \"{id}\" does not exist"));
        }
        // An arrow pointing at a deleted element would dangle and crash the
        // renderer's lookup — remove any arrow that referenced it instead.
        self.arrow_links.retain(|a| a.from != id && a.to != id);
        self.fixed_arrows.retain(|a| a.from.as_deref() != Some(id) && a.to.as_deref() != Some(id));
        Ok(())
    }

    pub fn update_element_text(&mut self, id: &str, text: &str) -> Result<(), String> {
        if let Some(b) = self.boxes.iter_mut().find(|b| b.id == id) {
            let (lines, width, height) = size_box(text);
            let center = b.rect.center();
            b.lines = lines;
            b.rect = Rect::from_center_size(center, Vec2::new(width, height));
            return Ok(());
        }
        if let Some(c) = self.circles.iter_mut().find(|c| c.id == id) {
            let (lines, radius) = size_circle(text);
            c.lines = lines;
            c.radius = radius;
            return Ok(());
        }
        Err(format!("element \"{id}\" does not exist"))
    }

    /// Duplicates an element with a new id, slightly offset, and returns
    /// the new id so the caller can select it.
    pub fn duplicate_element(&mut self, id: &str) -> Result<String, String> {
        let new_id = unique_id_from(self, &format!("{id}_copy"));
        let offset = Vec2::new(DUPLICATE_OFFSET, DUPLICATE_OFFSET);

        if let Some(b) = self.boxes.iter().find(|b| b.id == id).cloned() {
            self.boxes.push(LayoutBox { id: new_id.clone(), lines: b.lines, rect: b.rect.translate(offset) });
            return Ok(new_id);
        }
        if let Some(c) = self.circles.iter().find(|c| c.id == id).cloned() {
            self.circles.push(LayoutCircle { id: new_id.clone(), lines: c.lines, center: c.center + offset, radius: c.radius });
            return Ok(new_id);
        }
        Err(format!("element \"{id}\" does not exist"))
    }

    /// Repositions an element next to another existing one. This is the
    /// only way an element's position can be nudged by an operation — there
    /// is no operation that accepts raw coordinates.
    pub fn move_element_near(&mut self, id: &str, near_id: &str) -> Result<(), String> {
        if !self.element_exists(id) {
            return Err(format!("element \"{id}\" does not exist"));
        }
        let near_center = self.shape_center(near_id).ok_or_else(|| format!("element \"{near_id}\" does not exist"))?;
        self.set_shape_center(id, near_center + Vec2::new(MOVE_NEAR_OFFSET, 0.0));
        Ok(())
    }

    pub fn add_arrow(&mut self, from: &str, to: &str, text: String) -> Result<(), String> {
        if !self.element_exists(from) {
            return Err(format!("arrow source \"{from}\" does not exist"));
        }
        if !self.element_exists(to) {
            return Err(format!("arrow target \"{to}\" does not exist"));
        }
        self.arrow_links.push(ArrowLink { from: from.to_string(), to: to.to_string(), text });
        Ok(())
    }

    pub fn remove_arrow(&mut self, from: &str, to: &str) -> Result<(), String> {
        let before = self.arrow_links.len();
        self.arrow_links.retain(|a| !(a.from == from && a.to == to));
        if self.arrow_links.len() != before {
            return Ok(());
        }
        let before_fixed = self.fixed_arrows.len();
        self.fixed_arrows.retain(|a| !(a.from.as_deref() == Some(from) && a.to.as_deref() == Some(to)));
        if self.fixed_arrows.len() != before_fixed {
            return Ok(());
        }
        Err(format!("no arrow from \"{from}\" to \"{to}\""))
    }

    /// Validates and applies a batch of AI-proposed operations, one at a
    /// time. Invalid operations are skipped (with a message returned to the
    /// caller) rather than aborting the whole batch or crashing.
    pub fn apply_operations(&mut self, operations: Vec<Operation>) -> Vec<String> {
        let mut messages = Vec::new();
        for operation in operations {
            let result = match operation {
                Operation::AddElement { element } => match element {
                    NewElement::Box { id, text } => self.add_box(id, text),
                    NewElement::Circle { id, text } => self.add_circle(id, text),
                },
                Operation::RemoveElement { id } => self.remove_element(&id),
                Operation::UpdateElement { id, text } => self.update_element_text(&id, &text),
                Operation::MoveElement { id, near } => match near {
                    Some(near_id) => self.move_element_near(&id, &near_id),
                    None => Err(format!("move_element for \"{id}\" had no \"near\" target, ignored")),
                },
                Operation::AddArrow { from, to, text } => self.add_arrow(&from, &to, text),
                Operation::RemoveArrow { from, to } => self.remove_arrow(&from, &to),
            };
            if let Err(message) = result {
                messages.push(format!("Invalid AI operation: {message}"));
            }
        }
        messages
    }
}

/// Appends a number until `candidate` isn't already used on the board.
fn unique_id_from(board: &LaidOutDiagram, candidate: &str) -> String {
    if !board.element_exists(candidate) {
        return candidate.to_string();
    }
    let mut counter = 2;
    loop {
        let attempt = format!("{candidate}{counter}");
        if !board.element_exists(&attempt) {
            return attempt;
        }
        counter += 1;
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
/// word boundaries.
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

fn size_box(text: &str) -> (Vec<String>, f32, f32) {
    let lines = wrap_text(text, BOX_MAX_WIDTH);
    let longest_line_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
    let content_width = longest_line_chars * CHAR_WIDTH_ESTIMATE + BOX_PADDING_X * 2.0;
    let width = content_width.clamp(BOX_MIN_WIDTH, BOX_MAX_WIDTH);
    let height = lines.len() as f32 * LINE_HEIGHT + BOX_PADDING_Y * 2.0;
    (lines, width, height)
}

fn size_circle(text: &str) -> (Vec<String>, f32) {
    let lines = wrap_text(text, CIRCLE_MIN_RADIUS * 2.4);
    let longest_line_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
    let needed_radius = (longest_line_chars * CHAR_WIDTH_ESTIMATE / 2.0 + 14.0).max(CIRCLE_MIN_RADIUS);
    (lines, needed_radius)
}

// ---------------------------------------------------------------
// Initial layout from a freshly generated/example Diagram
// ---------------------------------------------------------------

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
                from: Some(from.clone()),
                to: Some(to.clone()),
            });
            arrow_y += PINGPONG_ROW_GAP;
        }
    }

    LaidOutDiagram { title: diagram.title.clone(), boxes, circles, arrow_links: Vec::new(), fixed_arrows }
}