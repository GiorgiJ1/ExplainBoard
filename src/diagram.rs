// This file defines the shape of the JSON that Ollama must produce.
// serde reads the JSON's "type" field and picks the matching variant below.

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
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        text: String,
    },
    Circle {
        id: String,
        x: i32,
        y: i32,
        radius: i32,
        text: String,
    },
    Text {
        id: String,
        x: i32,
        y: i32,
        text: String,
    },
    Arrow {
        from: String,
        to: String,
        #[serde(default)]
        text: String,
    },
}