use eframe::egui;
use eframe::epaint::{Color32, FontId};
use egui::{ColorImage, Context, RichText, TextureHandle, TextureOptions, Ui};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, mpsc};
use super::views::View;
use crate::config::Config;
use crate::controller::{AppEvent, Controller};
use crate::media::frame_handler::Frame;
use crate::ui::GUIError as Error;

/// Application state and UI controller for the `RoomRTC` GUI.
///
/// This struct holds the UI view state, the controller used to manage
/// the underlying session and media pipelines, the configuration, and
/// the channels used to receive frames and events from background
/// threads. It also stores textures used to render the local and
/// remote camera frames in the GUI.
///
/// The fields are intentionally public/internal to the module so the
/// eframe runtime and the controller code can interact with the
/// application. Create instances using [`RoomRTCApp::new`].
pub struct RoomRTCApp {
    view: View,
    controller: Controller,
    config: Arc<Config>,

    // Senders
    event_tx: Sender<AppEvent>,

    // Receivers
    event_rx: Receiver<AppEvent>,
    local_frame_rx:Option<Receiver<Frame>>,
    remote_frame_rx: Option<Receiver<Frame>>,

    // SDP
    our_offer: String,
    remote_sdp: String,
    our_answer: Option<String>,

    // Textures
    local_texture: Option<TextureHandle>,
    remote_texture: Option<TextureHandle>,

    // Error handling
    error_message: Option<String>,
}

impl RoomRTCApp {
    /// Create a new `RoomRTCApp` from the given configuration.
    ///
    /// Inputs:
    /// - `config`: application configuration used to construct the
    ///   `AppHandler` and to configure the session behavior.
    ///
    /// Outputs:
    /// - A fully initialised `RoomRTCApp` with channels created for
    ///   receiving local frames, remote frames and events. The
    ///   `AppHandler` is created and returned in a running state.
    #[must_use]
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        let (event_tx, event_rx) = mpsc::channel();

        let controller = match Controller::new(event_tx.clone(), &config) {
            Ok(c) => c,
            Err(e) => panic!("Failed to initialize controller: {}", e),
        };

        Self {
            view: View::default(),
            controller,
            config,
            event_tx,
            event_rx,
            local_frame_rx: None,
            remote_frame_rx: None,
            our_offer: String::new(),
            remote_sdp: String::new(),
            our_answer: None,
            local_texture: None,
            remote_texture: None,
            error_message: None,
        }
    }
}

impl eframe::App for RoomRTCApp {
    /// eframe application update callback.
    ///
    /// This method is called each frame by the `eframe` runtime. It
    /// performs the following responsibilities:
    /// - Drains and handles any incoming events signalled on
    ///   `rx_event`.
    /// - Renders the current `view` by delegating to the view-specific
    ///   helper methods (menu, connection, call, error).
    /// - Requests a repaint so the UI remains responsive while video
    ///   frames arrive.
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);

            // Esto es momentaneo, despues deberiamos tener un thread de eventos en la gui
            while self.event_rx.try_recv().is_ok() {
                if let Err(e) = self.controller.hang_down() {
                    self.error_message = Some(format!("Failed to stop camera: {e}"));
                } else {
                    self.error_message = Some("Call ended by remote peer".to_string());
                }
                self.reset();
                self.view = View::Error;
            }

            match self.view {
                View::Menu => self.show_menu(ui),
                View::Connection => self.show_connection(ui),
                View::Call => self.show_call(ctx, ui),
                View::Error => self.show_error(ui),
            }

            ctx.request_repaint();
        });
    }
}

impl RoomRTCApp {
    /// Render the main menu view.
    ///
    /// Presents two main actions to the user: create a call (generate
    /// an offer) or join a call (prepare to answer an offer). Button
    /// clicks update the stored SDP strings and switch the app view
    /// to the connection screen.
    fn show_menu(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            let create_btn =
                egui::Button::new("Create Call (Offer)").min_size(egui::vec2(200.0, 40.0));
            if ui.add_sized([200.0, 40.0], create_btn).clicked() {
                self.our_offer = self.controller.get_sdp_offer();
                self.remote_sdp = String::new();
                self.our_answer = None;
                self.view = View::Connection;
            }

            ui.add_space(10.0);

            let join_btn =
                egui::Button::new("Join Call (Answer)").min_size(egui::vec2(200.0, 40.0));
            if ui.add_sized([200.0, 40.0], join_btn).clicked() {
                self.our_offer = String::new();
                self.remote_sdp = String::new();
                self.our_answer = None;
                self.view = View::Connection;
            }
        });
    }

    /// Render the connection setup view.
    ///
    /// Depending on whether the local side is the offerer or the
    /// answerer, this shows the appropriate flow (see
    /// `offerer_flow` and `answerer_flow`). A Cancel button returns
    /// to the main menu.
    fn show_connection(&mut self, ui: &mut Ui) {
        if self.our_offer.is_empty() {
            self.answerer_flow(ui);
        } else {
            self.offerer_flow(ui);
        }

        if ui.button("Cancel").clicked() {
            self.view = View::Menu;
        }
    }

    /// Render the active call view.
    ///
    /// Displays the local and remote camera textures side-by-side,
    /// provides a button to end the call, and keeps the UI repaint
    /// requested so video updates are shown smoothly.
    fn show_call(&mut self, ctx: &Context, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Llamada");
            ui.add_space(20.0);

            if let Err(e) = self.update_video_textures(ctx) {
                self.error_message = Some(e.to_string());
                self.reset();
                self.view = View::Error;
                return;
            }

            ui.horizontal_centered(|ui| {
                ui.vertical(|ui| {
                    self.show_local_camera(ui);
                    ctx.request_repaint();
                });
                ui.vertical(|ui| {
                    self.show_remote_camera(ui);
                    ctx.request_repaint();
                });
            });

            let exit_btn = egui::Button::new("Finalizar llamada").min_size(egui::vec2(150.0, 40.0));
            if ui.add_sized([150.0, 40.0], exit_btn).clicked() {
                if let Err(e) = self.controller.hang_down() {
                    eprintln!("{e}");
                }
                self.reset();
                self.view = View::Menu;
            }
        });
    }

    /// UI flow for the offerer (call creator).
    ///
    /// Shows the generated offer (to copy and send to the remote),
    /// and accepts the pasted remote answer. When an answer is
    /// provided and the user clicks Connect, the controller processes
    /// the answer and the call is started.
    fn offerer_flow(&mut self, ui: &mut Ui) {
        ui.heading("You are the Offerer");
        ui.separator();
        ui.label("1. Copy your offer and send it to the other user:");
        ui.add(egui::TextEdit::multiline(&mut self.our_offer.clone()));
        ui.separator();

        ui.label("2. Paste the remote user's answer below:");
        ui.add(egui::TextEdit::multiline(&mut self.remote_sdp).hint_text("Paste SDP Answer..."));

        if !self.remote_sdp.is_empty() && ui.button("Connect").clicked() {
            match self.controller.process_answer(&self.remote_sdp) {
                Ok(()) => {
                    match self.controller.start_call() {
                        Ok((local_frame_rx, remote_frame_rx)) => {
                            self.local_frame_rx = Some(local_frame_rx);
                            self.remote_frame_rx = Some(remote_frame_rx);
                            self.view = View::Call;
                        },
                        Err(e) => {
                            self.error_message = Some(format!("Failed to start call: {e}"));
                            self.view = View::Error;
                        }
                    }
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to process answer: {e}"));
                    self.view = View::Error;
                }
            }
        }
    }

    /// UI flow for the answerer (call joiner).
    ///
    /// Accepts an offer (paste), optionally generates an answer that
    /// the user can copy and send back, and allows joining the call
    /// once the answer is generated and the user confirms.
    fn answerer_flow(&mut self, ui: &mut Ui) {
        ui.heading("You are the Answerer");
        ui.separator();
        ui.label("1. Paste the remote user's offer below:");
        ui.add(egui::TextEdit::multiline(&mut self.remote_sdp).hint_text("Paste SDP Offer..."));

        if self.our_answer.is_none()
            && !self.remote_sdp.is_empty()
            && ui.button("Generate Answer").clicked()
        {
            match self.controller.process_offer(&self.remote_sdp) {
                Ok(answer_str) => self.our_answer = Some(answer_str),
                Err(e) => {
                    self.error_message = Some(format!("Failed to process offer: {e}"));
                    self.view = View::Error;
                }
            }
        }

        if let Some(answer_str) = &self.our_answer {
            ui.separator();
            ui.label("2. Copy your answer and send it back:");
            ui.add(egui::TextEdit::multiline(&mut answer_str.clone()));
            ui.label("Connection established. Waiting for remote...");

            let join_btn = egui::Button::new("Join Call");
            if ui.add(join_btn).clicked() {
                match self.controller.start_call() {
                    Ok((local_frame_rx, remote_frame_rx)) => {
                        self.local_frame_rx = Some(local_frame_rx);
                        self.remote_frame_rx = Some(remote_frame_rx);
                        self.view = View::Call;
                    },
                    Err(e) => {
                        self.error_message = Some(format!("Failed to start call: {e}"));
                        self.view = View::Error;
                    }
                }
            }
        }
    }

    /// Update textures used to render camera frames.
    ///
    /// This function polls the `rx_local` and `rx_remote` channels
    /// and updates the corresponding `TextureHandle`s so the GUI
    /// can display the newest frames.
    fn update_video_textures(&mut self, ctx: &Context) -> Result<(), Error> {
        let local_frame_rx = self.local_frame_rx.as_ref().ok_or(Error::EmptyReceiver)?;
        let remote_frame_rx = self.remote_frame_rx.as_ref().ok_or(Error::EmptyReceiver)?;

        update_camera_view(ctx, &local_frame_rx, &mut self.local_texture, "local_camera");
        update_camera_view(ctx, &remote_frame_rx, &mut self.remote_texture, "remote_camera");
        
        Ok(())
    }

    /// Show the local camera image in the UI.
    ///
    /// If a texture exists, the image is resized preserving aspect
    /// ratio. Otherwise, a placeholder label is displayed.
    fn show_local_camera(&self, ui: &mut Ui) {
        if let Some(texture) = &self.local_texture {
            let size = texture.size_vec2();
            let aspect_ratio = size.x / size.y;
            let desired_height = 240.0;
            let desired_width = desired_height * aspect_ratio;

            let image = egui::Image::new(texture)
                .fit_to_exact_size(egui::vec2(desired_width, desired_height));
            ui.add(image);
        } else {
            ui.label("No se recibió video todavía...");
        }
    }

    /// Show the remote camera image in the UI.
    ///
    /// Behaviour mirrors `show_local_camera` but renders the remote
    /// participant's video.
    fn show_remote_camera(&self, ui: &mut Ui) {
        if let Some(texture) = &self.remote_texture {
            let size = texture.size_vec2();
            let aspect_ratio = size.x / size.y;
            let desired_height = 240.0;
            let desired_width = desired_height * aspect_ratio;

            let image = egui::Image::new(texture)
                .fit_to_exact_size(egui::vec2(desired_width, desired_height));
            ui.add(image);
        } else {
            ui.label("No se recibió video todavía...");
        }
    }

    /// Render the error view shown when the call ended unexpectedly.
    ///
    /// Provides a single action to go back to the main menu.
    fn show_error(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            let error_text = self
                .error_message
                .as_deref()
                .unwrap_or("An unknown error occurred");
            ui.label(
                RichText::new(error_text)
                    .color(Color32::RED)
                    .font(FontId::proportional(24.0)),
            );

            ui.add_space(20.0);

            if ui
                .add_sized([200.0, 40.0], egui::Button::new("Back to menu"))
                .clicked()
            {
                self.error_message = None;
                self.view = View::Menu;
            }
        });
    }

    /// Reset the application state to initial values.
    ///
    /// This recreates the internal channels and replaces the
    /// `AppHandler` with a fresh instance while dropping existing
    /// textures so a new call can be established cleanly.
    pub fn reset(&mut self) {
        self.local_frame_rx = None;
        self.remote_frame_rx = None;

        self.local_texture = None;
        self.remote_texture = None;

        self.controller = match Controller::new(self.event_tx.clone(), &self.config) {
            Ok(c) => c,
            Err(e) => {
                self.error_message = Some(format!("Failed to create controller: {e}"));
                self.view = View::Error;
                return;
            },
        };
    }
}

/// Poll a frame receiver and update (or create) the GUI texture.
///
/// This helper drains all available frames from `rx` and keeps only
/// the latest one, which is then converted into an `egui::ColorImage`
/// and set on the provided `TextureHandle`. If a texture does not
/// exist yet, it will be created via `Context::load_texture` using
/// `texture_name`.
///
/// Parameters:
/// - `ctx`: egui context used to create or update GPU textures.
/// - `rx`: receiver that provides `Frame` messages produced by the
///   media pipeline.
/// - `texture`: optional texture handle that will be updated/created.
/// - `texture_name`: name used for the texture when loading it into
///   the egui texture cache.
fn update_camera_view(
    ctx: &Context,
    rx: &Receiver<Frame>,
    texture: &mut Option<TextureHandle>,
    texture_name: &str,
) {
    let mut last_frame: Option<Frame> = None;
    while let Ok(frame) = rx.try_recv() {
        last_frame = Some(frame);
    }

    if let Some(frame) = last_frame {
        let color_img = ColorImage::from_rgb([frame.width, frame.height], &frame.data);
        if let Some(t) = texture {
            t.set(color_img, TextureOptions::default());
        } else {
            *texture = Some(ctx.load_texture(texture_name, color_img, TextureOptions::default()));
        }
    }
}
