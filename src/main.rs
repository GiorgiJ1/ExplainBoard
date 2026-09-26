mod diagram;
mod layout;
mod ollama;
mod operations;
mod renderer;
mod theme;

use diagram::Diagram;
use eframe::egui;
use layout::LaidOutDiagram;
use operations::OperationBatch;
use renderer::Camera;
use std::sync::mpsc::{self, Receiver};

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 780.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ExplainBoard",
        native_options,
        Box::new(|_creation_context| Ok(Box::new(ExplainBoardApp::default()))),
    )
}

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    Select,
    Pan,
}

struct ExplainBoardApp {
    prompt: String,
    layout: LaidOutDiagram,
    camera: Camera,
    status: String,
    ollama_ok: bool,
    tool: Tool,

    selected_id: Option<String>,
    drag_offset: egui::Vec2,
    fit_requested: bool,

    /// Text currently in the context panel's edit field, and which
    /// element id it belongs to (so switching selection refreshes it).
    edit_buffer: String,
    edit_buffer_for: Option<String>,
    /// Text currently in the "Ask AI" field.
    ask_ai_text: String,

    /// Board states to restore on Ctrl+Z / Ctrl+Shift+Z. A plain stack is
    /// enough for Day 3 — no command pattern needed.
    history: Vec<LaidOutDiagram>,
    redo_stack: Vec<LaidOutDiagram>,

    /// Set while a background thread is generating a brand new diagram.
    generate_receiver: Option<Receiver<Result<Diagram, String>>>,
    /// Set while a background thread is asking the AI to modify the board.
    operation_receiver: Option<Receiver<Result<OperationBatch, String>>>,
}

impl Default for ExplainBoardApp {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            layout: layout::layout(&diagram::example_tcp()),
            camera: Camera::default(),
            status: "Loaded an example diagram. Select an element to ask the AI about it.".to_string(),
            ollama_ok: true,
            tool: Tool::Select,
            selected_id: None,
            drag_offset: egui::Vec2::ZERO,
            fit_requested: true,
            edit_buffer: String::new(),
            edit_buffer_for: None,
            ask_ai_text: String::new(),
            history: Vec::new(),
            redo_stack: Vec::new(),
            generate_receiver: None,
            operation_receiver: None,
        }
    }
}

impl ExplainBoardApp {
    fn busy(&self) -> bool {
        self.generate_receiver.is_some() || self.operation_receiver.is_some()
    }

    // ---------------------------------------------------------------
    // History
    // ---------------------------------------------------------------

    /// Snapshots the current board before any mutating action, so it can be
    /// restored with undo. Call this FIRST, before changing self.layout.
    fn push_history(&mut self) {
        self.history.push(self.layout.clone());
        self.redo_stack.clear();
        if self.history.len() > 30 {
            self.history.remove(0); // keep the stack from growing forever
        }
    }

    fn undo(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.redo_stack.push(std::mem::replace(&mut self.layout, previous));
            self.selected_id = None;
            self.status = "Undid last change.".to_string();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.history.push(std::mem::replace(&mut self.layout, next));
            self.selected_id = None;
            self.status = "Redid change.".to_string();
        }
    }

    // ---------------------------------------------------------------
    // Board-changing actions
    // ---------------------------------------------------------------

    fn load_diagram(&mut self, diagram: Diagram, status_prefix: &str) {
        self.push_history();
        let diagram = diagram.sanitize();
        let warnings = diagram.validate();
        self.layout = layout::layout(&diagram);
        self.selected_id = None;
        self.fit_requested = true;
        self.status = if warnings.is_empty() {
            format!("{status_prefix}.")
        } else {
            format!("{status_prefix}, with warnings: {}", warnings.join("; "))
        };
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selected_id.clone() else { return };
        self.push_history();
        match self.layout.remove_element(&id) {
            Ok(()) => {
                self.selected_id = None;
                self.status = "Deleted.".to_string();
            }
            Err(message) => {
                self.history.pop(); // nothing changed; drop the unused snapshot
                self.status = format!("Could not delete: {message}");
            }
        }
    }

    fn duplicate_selected(&mut self) {
        let Some(id) = self.selected_id.clone() else { return };
        self.push_history();
        match self.layout.duplicate_element(&id) {
            Ok(new_id) => {
                self.selected_id = Some(new_id);
                self.status = "Duplicated.".to_string();
            }
            Err(message) => {
                self.history.pop();
                self.status = format!("Could not duplicate: {message}");
            }
        }
    }

    fn save_edit(&mut self) {
        let Some(id) = self.selected_id.clone() else { return };
        self.push_history();
        match self.layout.update_element_text(&id, &self.edit_buffer) {
            Ok(()) => self.status = "Updated.".to_string(),
            Err(message) => {
                self.history.pop();
                self.status = format!("Could not update: {message}");
            }
        }
    }

    /// Kicks off a background thread that asks Ollama to modify the board
    /// around the selected element. Mirrors start_generate's threading
    /// pattern, just with a different request/response type.
    fn start_ask_ai(&mut self, request: String) {
        let Some(id) = self.selected_id.clone() else { return };
        let Some(text) = self.layout.element_text(&id) else { return };
        if self.busy() {
            return;
        }

        let board_summary = self.layout.describe();
        self.status = "Ollama thinking...".to_string();

        let (sender, receiver) = mpsc::channel();
        self.operation_receiver = Some(receiver);

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("failed to start async runtime");
            let result = runtime.block_on(ollama::request_operations(&board_summary, &id, &text, &request));
            let _ = sender.send(result);
        });
    }

    fn start_generate(&mut self) {
        let prompt = self.prompt.trim().to_string();
        if prompt.is_empty() {
            self.status = "Type something to explain first.".to_string();
            return;
        }
        if self.busy() {
            return;
        }

        self.status = "Generating...".to_string();

        let (sender, receiver) = mpsc::channel();
        self.generate_receiver = Some(receiver);

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("failed to start async runtime");
            let result = runtime.block_on(ollama::generate_diagram(&prompt));
            let _ = sender.send(result);
        });
    }

    // ---------------------------------------------------------------
    // Polling background AI work
    // ---------------------------------------------------------------

    fn poll_generate(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.generate_receiver else { return };
        match receiver.try_recv() {
            Ok(Ok(new_diagram)) => {
                self.ollama_ok = true;
                self.load_diagram(new_diagram, "Done");
                self.generate_receiver = None;
            }
            Ok(Err(error_message)) => {
                self.ollama_ok = false;
                self.status = format!("Error: {error_message}");
                self.generate_receiver = None;
            }
            Err(_not_ready_yet) => ctx.request_repaint(),
        }
    }

    fn poll_operations(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.operation_receiver else { return };
        match receiver.try_recv() {
            Ok(Ok(batch)) => {
                self.ollama_ok = true;
                // The board changes here, so it must be snapshotted first —
                // this is what makes Ctrl+Z undo an AI modification.
                self.push_history();
                let warnings = self.layout.apply_operations(batch.operations);
                self.status = if warnings.is_empty() {
                    "AI updated the diagram.".to_string()
                } else {
                    format!("AI updated the diagram. {}", warnings.join(" "))
                };
                self.operation_receiver = None;
            }
            Ok(Err(error_message)) => {
                self.ollama_ok = false;
                self.status = format!("Error: {error_message}");
                self.operation_receiver = None;
            }
            Err(_not_ready_yet) => ctx.request_repaint(),
        }
    }

    // ---------------------------------------------------------------
    // Keyboard shortcuts
    // ---------------------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return; // a text field is focused; don't steal its keystrokes
        }

        let (escape, delete, backspace, ctrl_z, ctrl_shift_z, ctrl_d) = ctx.input(|input| {
            let ctrl = input.modifiers.command;
            (
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::Delete),
                input.key_pressed(egui::Key::Backspace),
                ctrl && !input.modifiers.shift && input.key_pressed(egui::Key::Z),
                ctrl && input.modifiers.shift && input.key_pressed(egui::Key::Z),
                ctrl && input.key_pressed(egui::Key::D),
            )
        });

        if escape {
            self.selected_id = None;
        }
        if (delete || backspace) && self.selected_id.is_some() {
            self.delete_selected();
        }
        if ctrl_shift_z {
            self.redo();
        } else if ctrl_z {
            self.undo();
        }
        if ctrl_d && self.selected_id.is_some() {
            self.duplicate_selected();
        }
    }
}

impl eframe::App for ExplainBoardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        self.poll_generate(ctx);
        self.poll_operations(ctx);
        self.handle_shortcuts(ctx);

        self.top_bar(ctx);
        self.toolbar(ctx);
        self.context_panel(ctx);
        self.canvas(ctx);
    }
}

impl ExplainBoardApp {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").frame(theme::dark_frame()).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("ExplainBoard").color(theme::TEXT_ON_DARK).strong().size(16.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (dot_color, status_text) = if self.busy() {
                        (theme::MUTED_TEXT, "Ollama thinking...")
                    } else if !self.ollama_ok {
                        (theme::ERROR_RED, "Ollama offline")
                    } else {
                        (theme::OK_GREEN, "Ready")
                    };
                    ui.colored_label(dot_color, "●");
                    ui.label(egui::RichText::new(status_text).color(theme::MUTED_TEXT));
                });
            });

            ui.add_space(8.0);

            egui::Frame::NONE
                .fill(theme::SURFACE)
                .corner_radius(10.0)
                .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if self.generate_receiver.is_some() {
                            ui.add(egui::Spinner::new().size(16.0));
                            ui.label(egui::RichText::new("Generating explanation...").color(theme::MUTED_TEXT));
                        } else {
                            let available = (ui.available_width() - 34.0).max(20.0);
                            let text_response = ui.add_sized(
                                [available, 22.0],
                                egui::TextEdit::singleline(&mut self.prompt)
                                    .hint_text("Explain how recursion works...")
                                    .frame(false),
                            );
                            let submitted = text_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.add_enabled(!self.busy(), egui::Button::new("↗")).clicked() || (submitted && !self.busy()) {
                                self.start_generate();
                            }
                        }
                    });
                });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Try:").color(theme::MUTED_TEXT).small());
                if ui.small_button("TCP handshake").clicked() {
                    self.load_diagram(diagram::example_tcp(), "Loaded");
                }
                if ui.small_button("DNS").clicked() {
                    self.load_diagram(diagram::example_dns(), "Loaded");
                }
                if ui.small_button("Magnetic force").clicked() {
                    self.load_diagram(diagram::example_physics(), "Loaded");
                }
            });

            ui.add_space(6.0);
            let status_color = if self.status.starts_with("Error") { theme::ERROR_RED } else { theme::MUTED_TEXT };
            ui.label(egui::RichText::new(&self.status).color(status_color).small());
        });
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("toolbar").frame(theme::dark_frame()).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.tool == Tool::Select, "Select").clicked() {
                    self.tool = Tool::Select;
                }
                if ui.selectable_label(self.tool == Tool::Pan, "Hand").clicked() {
                    self.tool = Tool::Pan;
                }
                ui.separator();
                if ui.button("+").clicked() {
                    self.camera.zoom = (self.camera.zoom * 1.2).min(5.0);
                }
                if ui.button("−").clicked() {
                    self.camera.zoom = (self.camera.zoom / 1.2).max(0.1);
                }
                if ui.button("Fit").clicked() {
                    self.fit_requested = true;
                }
                if ui.button("Reset").clicked() {
                    self.camera = Camera::default();
                }
                if ui.button("Clear").clicked() {
                    self.push_history();
                    self.layout = layout::layout(&Diagram { title: String::new(), elements: Vec::new() });
                    self.selected_id = None;
                    self.status = "Cleared.".to_string();
                }
                ui.separator();
                if ui.add_enabled(!self.history.is_empty(), egui::Button::new("Undo")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new("Redo")).clicked() {
                    self.redo();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{:.0}%", self.camera.zoom * 100.0)).color(theme::TEXT_ON_DARK));
                });
            });
        });
    }

    fn context_panel(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected_id.clone() else { return };
        let busy = self.busy();

        egui::SidePanel::right("context_panel")
            .resizable(false)
            .min_width(260.0)
            .frame(theme::dark_frame())
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&id).color(theme::TEXT_ON_DARK).strong().size(15.0));
                ui.add_space(8.0);

                if self.edit_buffer_for.as_deref() != Some(id.as_str()) {
                    self.edit_buffer = self.layout.element_text(&id).unwrap_or_default();
                    self.edit_buffer_for = Some(id.clone());
                }

                ui.label(egui::RichText::new("Text").color(theme::MUTED_TEXT).small());
                ui.text_edit_singleline(&mut self.edit_buffer);
                if ui.button("Save text").clicked() {
                    self.save_edit();
                }

                ui.add_space(12.0);
                ui.label(egui::RichText::new("AI actions").color(theme::MUTED_TEXT).small());
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Explain")).clicked() {
                        self.start_ask_ai("Explain this element in more detail.".to_string());
                    }
                    if ui.add_enabled(!busy, egui::Button::new("Expand")).clicked() {
                        self.start_ask_ai("Expand this concept with 2 or 3 additional connected concepts.".to_string());
                    }
                });
                if ui.add_enabled(!busy, egui::Button::new("Simplify")).clicked() {
                    self.start_ask_ai("Simplify this concept for a first-year university student.".to_string());
                }

                ui.add_space(12.0);
                ui.label(egui::RichText::new("Ask AI about this element").color(theme::MUTED_TEXT).small());
                ui.text_edit_singleline(&mut self.ask_ai_text);
                let can_ask = !busy && !self.ask_ai_text.trim().is_empty();
                if ui.add_enabled(can_ask, egui::Button::new("Ask")).clicked() {
                    let request = self.ask_ai_text.trim().to_string();
                    self.ask_ai_text.clear();
                    self.start_ask_ai(request);
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Duplicate").clicked() {
                        self.duplicate_selected();
                    }
                    if ui.button("Delete").clicked() {
                        self.delete_selected();
                    }
                });
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Ctrl+D duplicate · Delete key removes · Esc deselects · Ctrl+Z undo · Ctrl+Shift+Z redo")
                        .color(theme::MUTED_TEXT)
                        .small(),
                );
            });
    }

    fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let canvas_rect = ui.available_rect_before_wrap();
            let response = ui.interact(canvas_rect, ui.id().with("canvas"), egui::Sense::click_and_drag());
            let canvas_origin = canvas_rect.min;

            if self.fit_requested {
                self.camera.fit(canvas_rect.size(), self.layout.bounds());
                self.fit_requested = false;
            }

            // --- Zoom: mouse wheel, centered on the cursor rather than the origin ---
            let scroll_amount = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll_amount != 0.0 && response.hovered() {
                if let Some(mouse_screen) = ctx.input(|i| i.pointer.hover_pos()) {
                    let mouse_world_before = self.camera.screen_to_world(canvas_origin, mouse_screen);
                    let zoom_factor = 1.0 + scroll_amount * 0.001;
                    self.camera.zoom = (self.camera.zoom * zoom_factor).clamp(0.1, 5.0);
                    let mouse_screen_after = self.camera.world_to_screen(canvas_origin, mouse_world_before);
                    self.camera.pan += mouse_screen - mouse_screen_after;
                }
            }

            // --- Pan: right-drag always pans; left-drag pans only in Hand mode ---
            let panning = response.dragged_by(egui::PointerButton::Secondary)
                || (self.tool == Tool::Pan && response.dragged_by(egui::PointerButton::Primary));
            if panning {
                self.camera.pan += response.drag_delta();
            }

            // --- Select and drag an element: left mouse button, Select mode only ---
            if self.tool == Tool::Select {
                let starting_drag = response.drag_started() && response.dragged_by(egui::PointerButton::Primary);
                if starting_drag || response.clicked() {
                    if let Some(mouse_screen) = response.interact_pointer_pos() {
                        let mouse_world = self.camera.screen_to_world(canvas_origin, mouse_screen);
                        self.selected_id = self.layout.hit_test(mouse_world);
                        if let Some(id) = self.selected_id.clone() {
                            if let Some(center) = self.layout.shape_center(&id) {
                                self.drag_offset = mouse_world - center;
                            }
                        }
                    }
                }

                if self.selected_id.is_some() && response.dragged_by(egui::PointerButton::Primary) {
                    if let Some(mouse_screen) = response.interact_pointer_pos() {
                        let mouse_world = self.camera.screen_to_world(canvas_origin, mouse_screen);
                        let id = self.selected_id.clone().unwrap();
                        self.layout.set_shape_center(&id, mouse_world - self.drag_offset);
                    }
                }
            }

            // --- Draw ---
            let painter = ui.painter_at(canvas_rect);
            painter.rect_filled(canvas_rect, 0.0, theme::CANVAS_BACKGROUND);
            renderer::draw_grid(&painter, canvas_rect, &self.camera);
            renderer::draw_title(&painter, canvas_origin, &self.layout, &self.camera);

            let mut arrows = self.layout.resolve_arrows();
            arrows.extend(self.layout.fixed_arrows.iter().cloned());
            renderer::draw_diagram(&painter, canvas_origin, &self.layout, &arrows, &self.camera, self.selected_id.as_deref());
        });
    }
}