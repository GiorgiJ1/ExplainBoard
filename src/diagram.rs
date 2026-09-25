// This file defines the *semantic* diagram Ollama sends us: element types,
// ids, text, and how they relate via arrows. It intentionally has NO
// coordinates, sizes, or colors — layout.rs decides where everything
// actually goes, and renderer.rs decides how it looks. This is the fix for
// "don't trust Ollama's coordinates blindly."

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Diagram {
    pub title: String,
    pub elements: Vec<Element>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Element {
    Box { id: String, text: String },
    Circle { id: String, text: String },
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
            Element::Arrow { .. } => None,
        }
    }
}

impl Diagram {
    /// Checks every arrow's "from"/"to" against ids that actually exist.
    /// Returns one message per problem, instead of the arrow just quietly
    /// failing to appear.
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

    /// Cleans up text so malformed or irrelevant content from the model
    /// (empty strings, stray "%d"-style format artifacts, etc.) never ends
    /// up rendered as if it were real educational content.
    pub fn sanitize(mut self) -> Self {
        for element in &mut self.elements {
            let text_field = match element {
                Element::Box { text, .. } => text,
                Element::Circle { text, .. } => text,
                Element::Arrow { text, .. } => text,
            };
            *text_field = text_field.trim().to_string();
            if looks_malformed(text_field) {
                text_field.clear();
            }
        }

        // A box/circle needs *some* label — fall back to its id if the text
        // ended up empty (either it was always empty, or we just cleared it
        // above). Arrows are fine with an empty label (no text shown).
        for element in &mut self.elements {
            if let Element::Box { id, text } | Element::Circle { id, text } = element {
                if text.is_empty() {
                    *text = id.replace('_', " ");
                }
            }
        }

        self
    }
}

fn looks_malformed(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    // Stray printf-style format specifiers or template artifacts are not
    // real educational content — drop them rather than displaying them.
    matches!(text, "%d" | "%s" | "%f" | "{}" | "undefined" | "null" | "NaN")
}

/// Hardcoded example diagrams, used to prove the renderer and layout engine
/// work correctly without needing Ollama at all.
pub fn example_tcp() -> Diagram {
    Diagram {
        title: "TCP Three-Way Handshake".to_string(),
        elements: vec![
            Element::Box { id: "client".to_string(), text: "Client".to_string() },
            Element::Box { id: "server".to_string(), text: "Server".to_string() },
            Element::Arrow { from: "client".to_string(), to: "server".to_string(), text: "SYN".to_string() },
            Element::Arrow { from: "server".to_string(), to: "client".to_string(), text: "SYN-ACK".to_string() },
            Element::Arrow { from: "client".to_string(), to: "server".to_string(), text: "ACK".to_string() },
        ],
    }
}

pub fn example_dns() -> Diagram {
    Diagram {
        title: "How DNS Works".to_string(),
        elements: vec![
            Element::Box { id: "browser".to_string(), text: "Browser".to_string() },
            Element::Box { id: "resolver".to_string(), text: "DNS Resolver".to_string() },
            Element::Box { id: "dns_server".to_string(), text: "DNS Server".to_string() },
            Element::Box { id: "ip".to_string(), text: "IP Address".to_string() },
            Element::Box { id: "website".to_string(), text: "Website".to_string() },
            Element::Arrow { from: "browser".to_string(), to: "resolver".to_string(), text: String::new() },
            Element::Arrow { from: "resolver".to_string(), to: "dns_server".to_string(), text: String::new() },
            Element::Arrow { from: "dns_server".to_string(), to: "ip".to_string(), text: String::new() },
            Element::Arrow { from: "ip".to_string(), to: "website".to_string(), text: String::new() },
        ],
    }
}

pub fn example_physics() -> Diagram {
    Diagram {
        title: "Magnetic Force on a Current-Carrying Wire".to_string(),
        elements: vec![
            Element::Box { id: "field".to_string(), text: "Magnetic Field".to_string() },
            Element::Box { id: "wire".to_string(), text: "Current-Carrying Wire".to_string() },
            Element::Box { id: "force".to_string(), text: "Magnetic Force".to_string() },
            Element::Arrow { from: "field".to_string(), to: "wire".to_string(), text: String::new() },
            Element::Arrow { from: "wire".to_string(), to: "force".to_string(), text: String::new() },
        ],
    }
}