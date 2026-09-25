mod diagram;
mod layout;
mod ollama;
mod renderer;
mod theme;

use diagram::Diagram;
use eframe::egui;
use layout::LaidOutDiagram;
use renderer::Camera;
use std::sync::mpsc::{self, Receiver};

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 750.0]),
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
    /// Id of the currently selected/dragged box or circle, if any.
    selected_id: Option<String>,
    /// World-space offset between the mouse and the dragged element's
    /// center, captured when the drag starts, so the shape doesn't "jump".
    drag_offset: egui::Vec2,
    /// Set for one frame whenever the camera should re-fit the current
    /// diagram (after loading a new one, or the Fit button).
    fit_requested: bool,
    /// Set while a background thread is waiting on Ollama. Polled each frame.
    result_receiver: Option<Receiver<Result<Diagram, String>>>,
}

impl Default for ExplainBoardApp {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            layout: layout::layout(&diagram::example_tcp()),
            camera: Camera::default(),
            status: "Loaded an example diagram. Type a prompt and press the arrow to ask Ollama for a new one."
                .to_string(),
            ollama_ok: true,
            tool: Tool::Select,
            selected_id: None,
            drag_offset: egui::Vec2::ZERO,
            fit_requested: true,
            result_receiver: None,
        }
    }
}

impl ExplainBoardApp {
    fn load_diagram(&mut self, diagram: Diagram, status_prefix: &str) {
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

    /// Kicks off a background thread that talks to Ollama, so the UI never
    /// freezes while waiting. The result comes back through result_receiver.
    fn start_generate(&mut self) {
        let prompt = self.prompt.trim().to_string();
        if prompt.is_empty() {
            self.status = "Type something to explain first.".to_string();
            return;
        }

        self.status = "Generating...".to_string();

        let (sender, receiver) = mpsc::channel();
        self.result_receiver = Some(receiver);

        std::thread::spawn(move || {
            // eframe's event loop is not async, so we spin up a small tokio
            // runtime just for this one request, run it to completion, and
            // send the result back over the channel.
            let runtime = tokio::runtime::Runtime::new().expect("failed to start async runtime");
            let result = runtime.block_on(ollama::generate_diagram(&prompt));
            let _ = sender.send(result); // ignore send errors (window may have closed)
        });
    }
}

impl eframe::App for ExplainBoardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        // Check whether the background Ollama request has finished yet.
        if let Some(receiver) = &self.result_receiver {
            match receiver.try_recv() {
                Ok(Ok(new_diagram)) => {
                    self.ollama_ok = true;
                    self.load_diagram(new_diagram, "Done");
                    self.result_receiver = None;
                }
                Ok(Err(error_message)) => {
                    self.ollama_ok = false;
                    self.status = format!("Error: {error_message}");
                    self.result_receiver = None;
                }
                Err(_not_ready_yet) => {
                    ctx.request_repaint(); // keep checking next frame
                }
            }
        }

        self.top_bar(ctx);
        self.toolbar(ctx);
        self.canvas(ctx);
    }
}

impl ExplainBoardApp {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").frame(theme::dark_frame()).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("ExplainBoard").color(theme::TEXT_ON_DARK).strong().size(16.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let dot_color = if self.ollama_ok { theme::OK_GREEN } else { theme::ERROR_RED };
                    ui.colored_label(dot_color, "●");
                    ui.label(egui::RichText::new("Ollama").color(theme::MUTED_TEXT));
                });
            });

            ui.add_space(8.0);

            let is_generating = self.result_receiver.is_some();
            egui::Frame::NONE
                .fill(theme::SURFACE)
                .corner_radius(10.0)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if is_generating {
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
                            if ui.button("↗").clicked() || submitted {
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
                    self.layout = layout::layout(&Diagram { title: String::new(), elements: Vec::new() });
                    self.selected_id = None;
                    self.status = "Cleared.".to_string();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{:.0}%", self.camera.zoom * 100.0)).color(theme::TEXT_ON_DARK));
                });
            });
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