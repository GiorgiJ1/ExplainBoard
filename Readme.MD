# ExplainBoard

[![Rust](https://github.com/YOUR_USERNAME/explainboard/actions/workflows/rust.yml/badge.svg)](https://github.com/YOUR_USERNAME/explainboard/actions/workflows/rust.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
![Rust edition](https://img.shields.io/badge/rust-2021-orange.svg)

An AI whiteboard application. You type something like "Explain the TCP
three-way handshake", a local Ollama model generates a structured diagram
description, and the Rust app parses and renders it.

**Day 1 status:** the pipeline is built and tested end-to-end as a terminal
tool — user prompt → Rust → local Ollama API → structured JSON → parsed and
printed to the terminal. The graphical whiteboard renderer is not built yet.

## Requirements

- Rust + Cargo
- [Ollama](https://ollama.com) running locally with a model pulled, e.g.
  `ollama pull llama3.2`

## Running it

```bash
cargo run
```

```
> Explain the TCP three-way handshake
```

Type `exit` to quit.

## How it works

1. You type a prompt into the terminal.
2. The Rust app sends it to Ollama's local HTTP API (`/api/generate`) with
   `"format": "json"` and a system prompt that defines a diagram schema.
3. Ollama returns JSON describing boxes, circles, text, and arrows.
4. Rust deserializes that JSON with `serde` and prints the diagram elements.

## License

MIT — see [LICENSE](LICENSE).