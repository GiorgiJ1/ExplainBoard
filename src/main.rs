mod diagram;
mod ollama;
mod renderer;

use diagram::Diagram;
use eframe::egui;
use renderer::Camera;
use std::sync::mpsc::{self, Receiver};

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ExplainBoard",
        native_options,
        Box::new(|_creation_context| Ok(Box::new(ExplainBoardApp::default()))),
    )
}

struct ExplainBoardApp {
    prompt: String,
    diagram: Diagram,
    camera: Camera,
    status: String,
    /// Index into diagram.elements of the element currently being dragged, if any.
    dragging: Option<usize>,
    /// World-space offset between the mouse and the dragged element's anchor,
    /// captured when the drag starts, so the shape doesn't "jump" to the cursor.
    drag_offset: egui::Vec2,
    /// Set while a background thread is waiting on Ollama. Polled each frame.
    result_receiver: Option<Receiver<Result<Diagram, String>>>,
}

impl Default for ExplainBoardApp {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            diagram: diagram::test_diagram(),
            camera: Camera::default(),
            status: "Loaded a hardcoded test diagram. Type a prompt and press Go to ask Ollama for a new one.".to_string(),
            dragging: None,
            drag_offset: egui::Vec2::ZERO,
            result_receiver: None,
        }
    }
}

impl ExplainBoardApp {
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
        // Check whether the background Ollama request has finished yet.
        if let Some(receiver) = &self.result_receiver {
            match receiver.try_recv() {
                Ok(Ok(new_diagram)) => {
                    let warnings = new_diagram.validate();
                    self.diagram = new_diagram;
                    self.status = if warnings.is_empty() {
                        "Done.".to_string()
                    } else {
                        format!("Done, with warnings: {}", warnings.join("; "))
                    };
                    self.result_receiver = None;
                }
                Ok(Err(error_message)) => {
                    self.status = format!("Error: {error_message}");
                    self.result_receiver = None;
                }
                Err(_not_ready_yet) => {
                    ctx.request_repaint(); // keep checking next frame
                }
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.heading("ExplainBoard");
            ui.horizontal(|ui| {
                ui.label("What do you want explained?");
                ui.text_edit_singleline(&mut self.prompt);
                if ui.button("Go").clicked() && self.result_receiver.is_none() {
                    self.start_generate();
                }
            });
            ui.label(&self.status);
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Clear").clicked() {
                    self.diagram = Diagram { title: String::new(), elements: Vec::new() };
                }
                if ui.button("Zoom In").clicked() {
                    self.camera.zoom = (self.camera.zoom * 1.2).min(5.0);
                }
                if ui.button("Zoom Out").clicked() {
                    self.camera.zoom = (self.camera.zoom / 1.2).max(0.1);
                }
                if ui.button("Reset View").clicked() {
                    self.camera = Camera::default();
                }
                ui.label("  scroll = zoom, right-drag = pan, left-drag a shape = move it");
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let canvas_rect = ui.available_rect_before_wrap();
            let response = ui.interact(canvas_rect, ui.id().with("canvas"), egui::Sense::click_and_drag());
            let canvas_origin = canvas_rect.min;

            // --- Zoom: mouse wheel, only while hovering the canvas ---
            let scroll_amount = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll_amount != 0.0 && response.hovered() {
                let zoom_factor = 1.0 + scroll_amount * 0.001;
                self.camera.zoom = (self.camera.zoom * zoom_factor).clamp(0.1, 5.0);
            }

            // --- Pan: drag with the right mouse button ---
            if response.dragged_by(egui::PointerButton::Secondary) {
                self.camera.pan += response.drag_delta();
            }

            // --- Drag a single element: left mouse button ---
            if response.drag_started() && response.dragged_by(egui::PointerButton::Primary) {
                if let Some(mouse_screen) = response.interact_pointer_pos() {
                    let mouse_world = self.camera.screen_to_world(canvas_origin, mouse_screen);
                    self.dragging = self
                        .diagram
                        .elements
                        .iter()
                        .position(|element| element.contains(mouse_world.x, mouse_world.y));

                    if let Some(index) = self.dragging {
                        if let Some((anchor_x, anchor_y)) = self.diagram.elements[index].anchor() {
                            self.drag_offset = egui::Vec2::new(mouse_world.x - anchor_x, mouse_world.y - anchor_y);
                        }
                    }
                }
            }

            if self.dragging.is_some() && response.dragged_by(egui::PointerButton::Primary) {
                if let Some(mouse_screen) = response.interact_pointer_pos() {
                    let mouse_world = self.camera.screen_to_world(canvas_origin, mouse_screen);
                    let index = self.dragging.unwrap();
                    self.diagram.elements[index].set_position(
                        mouse_world.x - self.drag_offset.x,
                        mouse_world.y - self.drag_offset.y,
                    );
                }
            }

            if response.drag_stopped() {
                self.dragging = None;
            }

            // --- Draw everything ---
            let painter = ui.painter_at(canvas_rect);
            painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_gray(245));
            renderer::draw_diagram(&painter, canvas_origin, &self.diagram, &self.camera);
        });
    }
}