// This file talks directly to Ollama's HTTP API. No Ollama-specific crate is
// used, so you can see exactly what a request/response looks like.

use crate::diagram::Diagram;
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