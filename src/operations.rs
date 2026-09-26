// This file defines the ONLY vocabulary the AI is allowed to use to change
// the whiteboard. It cannot send coordinates, colors, or raw commands — just
// these six operation types. layout.rs is responsible for validating and
// applying them; this file only describes their shape.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OperationBatch {
    pub operations: Vec<Operation>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    AddElement { element: NewElement },
    RemoveElement { id: String },
    UpdateElement { id: String, text: String },
    MoveElement {
        id: String,
        #[serde(default)]
        near: Option<String>,
    },
    AddArrow {
        from: String,
        to: String,
        #[serde(default)]
        text: String,
    },
    RemoveArrow { from: String, to: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NewElement {
    Box { id: String, text: String },
    Circle { id: String, text: String },
}