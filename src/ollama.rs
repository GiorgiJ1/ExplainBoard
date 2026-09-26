// This file talks directly to Ollama's HTTP API. No Ollama-specific crate is
// used, so you can see exactly what a request/response looks like.

use crate::diagram::Diagram;
use crate::operations::OperationBatch;
use serde::{Deserialize, Serialize};

const OLLAMA_URL: &str = "http://localhost:11434/api/generate";

// ---------------------------------------------------------------------
// CHANGE THIS if you want a different model.
// Run `ollama list` in your terminal to see what you have installed,
// and `ollama pull llama3.2` (or another name) if you need to get one.
// ---------------------------------------------------------------------
const MODEL_NAME: &str = "llama3.2";

// This is sent as Ollama's "system" prompt. It tells the model exactly
// what shape of JSON we expect back, and forbids prose/Markdown.
const SYSTEM_PROMPT: &str = r#"You are a diagram-generating assistant for a visual whiteboard application.

You will be given a request to explain a concept visually.

You must respond with ONLY valid JSON. Do not include any explanation,
commentary, or Markdown formatting. Do not wrap the JSON in ```json code
blocks. Output raw JSON only, starting with { and ending with }.

You describe WHAT the diagram contains and HOW its parts relate. You do NOT
choose coordinates, sizes, or colors — a separate layout system positions
everything automatically based on the relationships you describe.

The JSON must match this exact schema:

{
  "title": "string - a short, specific title for the diagram",
  "elements": [
    { "type": "box", "id": "string", "text": "string" },
    { "type": "circle", "id": "string", "text": "string" },
    { "type": "arrow", "from": "string (id of an existing box/circle)", "to": "string (id of an existing box/circle)", "text": "string (can be empty)" }
  ]
}

Rules:
- Every "box" and "circle" must have a short, unique "id" (lowercase, no
  spaces, e.g. "client", "dns_server").
- Every "arrow" must reference "from" and "to" ids that exist among the
  box/circle elements.
- Use "box" for most concepts, steps, and components. Use "circle" only for
  small standalone nodes (a single value, a point in space).
- Keep each element's "text" short — a few words, not a sentence.
- List elements in a sensible reading order: whatever a student would
  encounter first should appear first in the array.
- Prefer 3 to 6 elements total.
- Do NOT create a "text"-type element — it does not exist in this schema.
  Every piece of text must belong to a box, a circle, or an arrow's label.
- Output ONLY the JSON object. No prose before or after it.
"#;

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: String,
    system: &'a str,
    stream: bool,
    // Ollama's built-in JSON mode: it constrains generation so the
    // output is guaranteed to be syntactically valid JSON. It does NOT
    // guarantee our specific schema, which is why we still ask nicely
    // for the schema in the system prompt, and still handle parse
    // errors gracefully below.
    format: &'a str,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

pub async fn generate_diagram(user_prompt: &str) -> Result<Diagram, String> {
    let client = reqwest::Client::new();

    let request_body = OllamaRequest {
        model: MODEL_NAME,
        prompt: format!("Explain this as a visual diagram: {}", user_prompt),
        system: SYSTEM_PROMPT,
        stream: false,
        format: "json",
    };

    let http_response = client
        .post(OLLAMA_URL)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| {
            format!(
                "Could not reach Ollama at {}.\nIs Ollama running? Try `ollama serve`.\nDetails: {}",
                OLLAMA_URL, e
            )
        })?;

    if !http_response.status().is_success() {
        let status = http_response.status();
        let body = http_response.text().await.unwrap_or_default();
        return Err(format!(
            "Ollama returned an error status: {}\nBody: {}\nCheck that the model '{}' exists (`ollama list`).",
            status, body, MODEL_NAME
        ));
    }

    let ollama_response: OllamaResponse = http_response
        .json()
        .await
        .map_err(|e| format!("Could not parse Ollama's outer response as JSON: {}", e))?;

    let raw_json = ollama_response.response.trim();

    let diagram: Diagram = serde_json::from_str(raw_json).map_err(|e| {
        format!(
            "Ollama's reply was not valid diagram JSON.\n\nRaw text from the model:\n{}\n\nParse error: {}",
            raw_json, e
        )
    })?;

    Ok(diagram)
}

// ---------------------------------------------------------------------
// Day 3: modifying an EXISTING board instead of generating a new one.
// The model receives the current board (by id, not by pixel), which
// element is selected, and what the user asked for — and returns a list
// of operations rather than a whole diagram. See operations.rs for the
// operation types, and layout.rs for how they get validated and applied.
// ---------------------------------------------------------------------

const OPERATIONS_SYSTEM_PROMPT: &str = r#"You are an assistant that modifies an existing diagram on a visual whiteboard.

You will be given:
- The current whiteboard content (its elements and arrows, by id).
- Which element is currently selected.
- What the user is asking for.

You must respond with ONLY valid JSON: a list of operations to apply to the
whiteboard. Do not include explanation, commentary, or Markdown formatting.
Do not wrap the JSON in ```json code blocks. Output raw JSON only, starting
with { and ending with }.

You do NOT return a whole new diagram, and you do NOT choose coordinates —
Rust positions everything automatically. You only describe changes.

The JSON must match this exact schema:

{
  "operations": [
    { "type": "add_element", "element": { "type": "box", "id": "string", "text": "string" } },
    { "type": "add_element", "element": { "type": "circle", "id": "string", "text": "string" } },
    { "type": "remove_element", "id": "string" },
    { "type": "update_element", "id": "string", "text": "string" },
    { "type": "move_element", "id": "string", "near": "string (id of another existing element, optional)" },
    { "type": "add_arrow", "from": "string (existing id)", "to": "string (existing id)", "text": "string (can be empty)" },
    { "type": "remove_arrow", "from": "string (existing id)", "to": "string (existing id)" }
  ]
}

Rules:
- Preserve the existing diagram. Only change what the user's request requires.
- Every id you reference in remove_element, update_element, move_element,
  add_arrow, or remove_arrow must be one of the ids listed in the current
  whiteboard content, or an id you are adding earlier in this same operations list.
- New ids for add_element must be short, lowercase, unique, and not already
  used on the whiteboard.
- Prefer add_element + add_arrow over update_element when introducing a new
  idea. Use update_element only to change an existing element's own text.
- Keep new "text" fields short — a few words to a short sentence, not a paragraph.
- Return as few operations as needed to satisfy the request. Usually 1 to 4.
- Output ONLY the JSON object. No prose before or after it.
"#;

pub async fn request_operations(
    board_summary: &str,
    selected_id: &str,
    selected_text: &str,
    user_request: &str,
) -> Result<OperationBatch, String> {
    let client = reqwest::Client::new();

    let prompt = format!(
        "CURRENT WHITEBOARD:\n{board_summary}\nSELECTED ELEMENT:\nid: {selected_id}, text: \"{selected_text}\"\n\nUSER REQUEST:\n\"{user_request}\""
    );

    let request_body = OllamaRequest {
        model: MODEL_NAME,
        prompt,
        system: OPERATIONS_SYSTEM_PROMPT,
        stream: false,
        format: "json",
    };

    let http_response = client
        .post(OLLAMA_URL)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| {
            format!(
                "Could not reach Ollama at {}.\nIs Ollama running? Try `ollama serve`.\nDetails: {}",
                OLLAMA_URL, e
            )
        })?;

    if !http_response.status().is_success() {
        let status = http_response.status();
        let body = http_response.text().await.unwrap_or_default();
        return Err(format!(
            "Ollama returned an error status: {}\nBody: {}\nCheck that the model '{}' exists (`ollama list`).",
            status, body, MODEL_NAME
        ));
    }

    let ollama_response: OllamaResponse = http_response
        .json()
        .await
        .map_err(|e| format!("Could not parse Ollama's outer response as JSON: {}", e))?;

    let raw_json = ollama_response.response.trim();

    let batch: OperationBatch = serde_json::from_str(raw_json).map_err(|e| {
        format!(
            "Ollama's reply was not a valid operations list.\n\nRaw text from the model:\n{}\n\nParse error: {}",
            raw_json, e
        )
    })?;

    Ok(batch)
}