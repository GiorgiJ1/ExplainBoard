// This file defines the shape of the JSON that Ollama must produce, plus a
// few small helper methods the renderer and the drag-and-drop code rely on.
//
// Coordinates are f32 (not i32 like Day 1) because a graphical canvas needs
// fractional positions for smooth zooming and dragging.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Diagram {
    pub title: String,
    pub elements: Vec<Element>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Element {
    Box {
        id: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        text: String,
    },
    Circle {
        id: String,
        x: f32,
        y: f32,
        radius: f32,
        text: String,
    },
    Text {
        id: String,
        x: f32,
        y: f32,
        text: String,
    },
    Arrow {
        from: String,
        to: String,
        #[serde(default)]
        text: String,
    },
}

impl Element {
    /// This element's own id. Arrows don't have one — they're identified by
    /// their "from" and "to" fields instead.
    pub fn id(&self) -> Option<&str> {
        match self {
            Element::Box { id, .. } => Some(id),
            Element::Circle { id, .. } => Some(id),
            Element::Text { id, .. } => Some(id),
            Element::Arrow { .. } => None,
        }
    }

    /// The point other elements should draw arrows to/from. For a box this
    /// is its visual center, not its top-left corner.
    pub fn center(&self) -> Option<(f32, f32)> {
        match self {
            Element::Box { x, y, width, height, .. } => {
                Some((x + width / 2.0, y + height / 2.0))
            }
            Element::Circle { x, y, .. } => Some((*x, *y)),
            Element::Text { x, y, .. } => Some((*x, *y)),
            Element::Arrow { .. } => None,
        }
    }

    /// The raw (x, y) stored in the JSON. Dragging reads and writes this
    /// exact point, so it must stay in sync with `set_position`.
    pub fn anchor(&self) -> Option<(f32, f32)> {
        match self {
            Element::Box { x, y, .. } => Some((*x, *y)),
            Element::Circle { x, y, .. } => Some((*x, *y)),
            Element::Text { x, y, .. } => Some((*x, *y)),
            Element::Arrow { .. } => None,
        }
    }

    /// Moves a draggable element's (x, y). Arrows have no position of their
    /// own — they're just a line between two other elements — so this does
    /// nothing for them.
    pub fn set_position(&mut self, new_x: f32, new_y: f32) {
        match self {
            Element::Box { x, y, .. } => {
                *x = new_x;
                *y = new_y;
            }
            Element::Circle { x, y, .. } => {
                *x = new_x;
                *y = new_y;
            }
            Element::Text { x, y, .. } => {
                *x = new_x;
                *y = new_y;
            }
            Element::Arrow { .. } => {}
        }
    }

    /// Whether a world-space point falls inside this element. Used to figure
    /// out which element (if any) the mouse just clicked on.
    pub fn contains(&self, point_x: f32, point_y: f32) -> bool {
        match self {
            Element::Box { x, y, width, height, .. } => {
                point_x >= *x && point_x <= x + width && point_y >= *y && point_y <= y + height
            }
            Element::Circle { x, y, radius, .. } => {
                let dx = point_x - x;
                let dy = point_y - y;
                (dx * dx + dy * dy).sqrt() <= *radius
            }
            Element::Text { x, y, .. } => {
                // Text has no real width/height here, so give it a small
                // clickable box around its position.
                point_x >= x - 5.0 && point_x <= x + 60.0 && point_y >= y - 10.0 && point_y <= y + 10.0
            }
            Element::Arrow { .. } => false,
        }
    }
}

impl Diagram {
    /// Checks every arrow's "from"/"to" against the ids that actually exist
    /// in this diagram. Returns one message per problem found, so the
    /// caller can show them instead of the arrow just silently vanishing.
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        for element in &self.elements {
            if let Element::Arrow { from, to, .. } = element {
                if self.find_id(from).is_none() {
                    warnings.push(format!("Arrow references unknown element id \"{from}\""));
                }
                if self.find_id(to).is_none() {
                    warnings.push(format!("Arrow references unknown element id \"{to}\""));
                }
            }
        }
        warnings
    }

    fn find_id(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|element| element.id() == Some(id))
    }
}

/// A hardcoded diagram used to test the renderer without needing Ollama at
/// all. This is what the app shows the moment it starts up.
pub fn test_diagram() -> Diagram {
    Diagram {
        title: "TCP Three-Way Handshake (test diagram)".to_string(),
        elements: vec![
            Element::Box {
                id: "client".to_string(),
                x: 100.0,
                y: 150.0,
                width: 160.0,
                height: 80.0,
                text: "Client".to_string(),
            },
            Element::Box {
                id: "server".to_string(),
                x: 500.0,
                y: 150.0,
                width: 160.0,
                height: 80.0,
                text: "Server".to_string(),
            },
            Element::Arrow {
                from: "client".to_string(),
                to: "server".to_string(),
                text: "SYN".to_string(),
            },
            Element::Arrow {
                from: "server".to_string(),
                to: "client".to_string(),
                text: "SYN-ACK".to_string(),
            },
            Element::Arrow {
                from: "client".to_string(),
                to: "server".to_string(),
                text: "ACK".to_string(),
            },
        ],
    }
}