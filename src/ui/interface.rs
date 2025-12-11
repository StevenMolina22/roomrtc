use super::views::View;
use crate::config::Config;
use crate::controller::{AppEvent, Controller};
use crate::logger::Logger;
use crate::media::frame_handler::Frame;
use crate::session::sdp::SessionDescriptionProtocol;
use crate::ui::GUIError as Error;
use crate::user::UserStatus;
use eframe::egui;
use eframe::epaint::{Color32, FontId};
use egui::{ColorImage, Context, RichText, TextureHandle, TextureOptions, Ui};
use std::net::SocketAddr;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, mpsc};
use crate::transport::rtcp::CallStats;

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

    // Receivers
    event_rx: Receiver<AppEvent>,
    local_frame_rx: Option<Receiver<Frame>>,
    remote_frame_rx: Option<Receiver<Frame>>,

    // UserData
    username_buff: String,
    password_buff: String,

    // Textures
    local_texture: Option<TextureHandle>,
    remote_texture: Option<TextureHandle>,

    // Error handling
    error_msg: Option<String>,
    warning_msg: Option<String>,

    last_stats: Option<CallStats>
}

impl RoomRTCApp {
    /// Create a new `RoomRTCApp` from the given configuration.
    ///
    /// Inputs:
    /// - `config`: application configuration used to construct the
    ///   `Controller` and to configure the session behavior.
    ///
    /// Outputs:
    /// - A fully initialised `RoomRTCApp` with channels created for
    ///   receiving local frames, remote frames and events. The
    ///   `Controller` is created and returned in a running state.
    #[must_use]
    pub fn new(config: Config, server_address: SocketAddr, logger: Logger) -> Self {
        let config = Arc::new(config);
        let (event_tx, event_rx) = mpsc::channel();

        let mut controller = match Controller::new(
            event_tx,
            &config,
            server_address,
            logger.context("Controller"),
        ) {
            Ok(c) => c,
            Err(e) => panic!("Failed to initialize controller: {e}"),
        };

        if controller.initial_handshake().is_err() {
            std::process::exit(1);
        }

        Self {
            view: View::default(),
            controller,
            event_rx,
            local_frame_rx: None,
            remote_frame_rx: None,
            username_buff: String::new(),
            password_buff: String::new(),
            local_texture: None,
            remote_texture: None,
            error_msg: None,
            warning_msg: None,
            last_stats: None
        }
    }
}

impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);
            loop {
                match self.event_rx.try_recv() {
                    Ok(event) => self.handle_event(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.view = View::FatalError;
                        break;
                    }
                }
            }

            self.show_warning_popup(ctx);

            match &self.view {
                View::Welcome => self.show_welcome(ui),
                View::SignUp => self.show_sign_up(ui),
                View::LogIn => self.show_log_in(ui),
                View::Calling(peer_username) => self.show_calling(peer_username.clone(), ui),
                View::CallIncoming(peer_username, sdp_offer) => {
                    self.show_call_incoming(peer_username.clone(), sdp_offer.clone(), ui);
                }
                View::CallHub => self.show_call_hub(ui),
                View::Call(username, peer_username) => {
                    self.show_call(username.clone(), peer_username.clone(), ctx, ui);
                }
                View::CallEnded => self.show_call_ended(ui),
                View::Error => self.show_error(ui),
                View::FatalError => self.show_fatal_error(ui),
                View::FullServer => self.show_full_server(ui)
            }

            ctx.request_repaint();
        });
    }
}

impl RoomRTCApp {
    fn show_welcome(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            let signup_btn = egui::Button::new("Sign Up").min_size(egui::vec2(200.0, 40.0));
            if ui.add_sized([200.0, 40.0], signup_btn).clicked() {
                self.view = View::SignUp;
            }

            ui.add_space(10.0);

            let login_btn = egui::Button::new("Log In").min_size(egui::vec2(200.0, 40.0));
            if ui.add_sized([200.0, 40.0], login_btn).clicked() {
                self.view = View::LogIn;
            }
        });
    }

    fn show_sign_up(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Sign Up");
            ui.add(egui::TextEdit::singleline(&mut self.username_buff).hint_text("Username"));
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.password_buff).password(true).hint_text("Password"));
        });

        if !self.username_buff.is_empty()
            && !self.password_buff.is_empty()
            && ui.button("Sign Up").clicked()
        {
            match self
                .controller
                .sign_up(self.username_buff.clone(), self.password_buff.clone())
            {
                Ok(()) => self.view = View::LogIn,
                Err(e) => self.warning_msg = Some(e.to_string()),
            }
            self.username_buff.clear();
            self.password_buff.clear();
        }

        if ui.button("Back").clicked() {
            self.username_buff.clear();
            self.password_buff.clear();
            self.view = View::Welcome;
        }
    }

    fn show_log_in(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Log In");
            ui.add(egui::TextEdit::singleline(&mut self.username_buff).hint_text("Username"));
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.password_buff).password(true).hint_text("Password"));
        });

        if !self.username_buff.is_empty()
            && !self.password_buff.is_empty()
            && ui.button("Log In").clicked()
        {
            let username = self.username_buff.clone();
            let password = self.password_buff.clone();
            match self.controller.log_in(&username, &password) {
                Ok(()) => self.view = View::CallHub,
                Err(e) => self.warning_msg = Some(e.to_string()),
            }
            self.username_buff.clear();
            self.password_buff.clear();
        }

        if ui.button("Back").clicked() {
            self.username_buff.clear();
            self.password_buff.clear();
            self.view = View::Welcome;
        }
    }

    fn show_call_hub(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                    let username = match self.controller.get_username() {
                        Ok(username) => username,
                        Err(_) => {
                            self.view = View::FatalError;
                            return;
                    }
                };

                ui.label(username);

                if ui.button("Log Out").clicked() {
                    match self.controller.log_out() {
                        Ok(()) => {
                            self.view = View::Welcome;
                        }
                        Err(_) => {
                            self.view = View::Error;
                        }
                    }
                }
            });

            ui.add_space(10.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);

                    let users_status = match self.controller.get_users_status() {
                        Ok(users_status) => users_status,
                        Err(_) => {
                            self.view = View::FatalError;
                            return;
                        }
                    };

                    for (username, status) in &users_status {
                        ui.horizontal(|ui| {
                            ui.label(username);
                            ui.label(status.to_string());

                            if *status == UserStatus::Available && ui.button("Call").clicked() {
                                if let Err(e) = self.controller.call(username) {
                                    self.warning_msg = Some(e.to_string());
                                } else {
                                    self.view = View::Calling(username.clone());
                                }
                            }
                        });
                        ui.separator();
                    }
                });
        });
    }

    fn show_calling(&mut self, peer_username: String, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading(format!("Calling {peer_username}"));
            ui.separator();
            ui.label("Connecting…");
            ui.separator();
        });
    }

    fn show_call_incoming(
        &mut self,
        peer_username: String,
        sdp_offer: SessionDescriptionProtocol,
        ui: &mut Ui,
    ) {
        ui.vertical_centered(|ui| {
            ui.separator();
            ui.heading(format!("{peer_username} is calling you..."));
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Accept").clicked() {
                    match self.controller.accept_call(peer_username.clone(), &sdp_offer) {
                        Ok((local_frame_rx, remote_frame_rx)) => {
                            match self.controller.get_username() {
                                Ok(username) => {
                                    self.local_frame_rx = Some(local_frame_rx);
                                    self.remote_frame_rx = Some(remote_frame_rx);
                                    self.view = View::Call(username, peer_username.clone());
                                }
                                Err(e) => {
                                    self.error_msg = Some(e.to_string());
                                    self.view = View::Error;
                                }
                            }
                        }
                        Err(e) => {
                            self.warning_msg = Some(e.to_string());
                            self.view = View::CallHub;
                        }
                    }
                }
                if ui.button("Decline").clicked() {
                    match self.controller.reject_call(peer_username) {
                        Ok(()) => self.view = View::CallHub,
                        Err(e) => self.warning_msg = Some(e.to_string()),
                    }
                }
            });
            ui.separator();
        });
    }

    fn show_call(&mut self, username: String, peer_username: String, ctx: &Context, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Call");
            ui.add_space(20.0);

            if let Err(e) = self.update_video_textures(ctx) {
                self.warning_msg = Some(e.to_string());
                if self.controller.hang_up().is_err() {
                    self.view = View::FatalError;
                } else {
                    self.reset_after_call();
                    self.view = View::CallHub;
                }
                return;
            }

            ui.horizontal_centered(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(username).size(16.0).strong());

                    ui.add_space(4.0);
                    self.show_local_camera(ui);
                    ctx.request_repaint();
                });
                ui.vertical(|ui| {
                    ui.label(RichText::new(peer_username.to_string()).size(16.0).strong());

                    ui.add_space(4.0);
                    self.show_remote_camera(ui);
                    ctx.request_repaint();
                });
            });

            let exit_btn = egui::Button::new("End call").min_size(egui::vec2(150.0, 40.0));
            if ui.add_sized([150.0, 40.0], exit_btn).clicked() {
                if self.controller.hang_up().is_err() {
                    self.view = View::FatalError;
                } else {
                    self.reset_after_call();
                    self.view = View::CallHub;
                }
            }

            ui.separator();
            if let Some(stats) = &self.last_stats {
                ui.scope(|ui| {
                    ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
                    ui.label(RichText::new("📊 Network Stats").strong());

                    ui.horizontal(|ui| {
                        ui.label(format!("Jitter: {}ms", stats.remote_receiver.jitter));
                        ui.label(format!("Lost: {} pkts", stats.remote_receiver.packets_lost));
                    });

                    ui.horizontal(|ui| {
                        ui.label(format!("Sent: {}", stats.local_sender.packets_sent));
                        ui.label(format!("Received: {}", stats.local_receiver.packets_received));
                    });
                });
            } else {
                ui.label("Esperando reporte RTCP...");
            }
        });
    }

    fn show_call_ended(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Call Ended");
            ui.add_space(10.0);

            if ui.button("Back to menu").clicked() {
                self.view = View::CallHub;
            }
        });
    }

    fn show_full_server(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            ui.label(
                RichText::new("Server is full. Try again soon!")
                    .color(Color32::RED)
                    .font(FontId::proportional(24.0)),
            );

            ui.add_space(20.0);

            if ui
                .add_sized([200.0, 40.0], egui::Button::new("Close App"))
                .clicked()
            {
                std::process::exit(0);
            }
        });
    }

    fn update_video_textures(&mut self, ctx: &Context) -> Result<(), Error> {
        let local_frame_rx = self.local_frame_rx.as_ref().ok_or(Error::EmptyReceiver)?;
        let remote_frame_rx = self.remote_frame_rx.as_ref().ok_or(Error::EmptyReceiver)?;

        update_camera_view(ctx, local_frame_rx, &mut self.local_texture, "local_camera");
        update_camera_view(
            ctx,
            remote_frame_rx,
            &mut self.remote_texture,
            "remote_camera",
        );

        Ok(())
    }

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
            ui.label("Waiting for local camera...");
        }
    }

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
            ui.label("Waiting for remote camera...");
        }
    }

    fn show_warning_popup(&mut self, ctx: &Context) {
        let msg = match &self.warning_msg {
            Some(m) => m.clone(),
            None => return,
        };

        egui::TopBottomPanel::top("error_popup").show(ctx, |ui| {
            ui.colored_label(Color32::WHITE, "");

            ui.visuals_mut().widgets.noninteractive.bg_fill = Color32::RED;

            ui.horizontal(|ui| {
                ui.label(RichText::new(msg.clone()).color(Color32::WHITE).strong());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✖").clicked() {
                        self.warning_msg = None;
                    }
                });
            });
            ui.add_space(4.0);
        });
    }

    fn show_error(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            let error_text = self.error_msg.as_deref().unwrap_or("An unknown error occurred");
            ui.label(
                RichText::new(error_text)
                    .color(Color32::RED)
                    .font(FontId::proportional(24.0)),
            );

            ui.add_space(20.0);

            if ui
                .add_sized([200.0, 40.0], egui::Button::new("Ok"))
                .clicked()
            {
                self.error_msg = None;
                self.reset_after_call();
                self.view = View::CallHub;
            }
        });
    }

    fn show_fatal_error(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            ui.label(
                RichText::new("An unexpected error occurred")
                    .color(Color32::RED)
                    .font(FontId::proportional(24.0)),
            );

            ui.add_space(20.0);

            if ui
                .add_sized([200.0, 40.0], egui::Button::new("Close App"))
                .clicked()
            {
                std::process::exit(0);
            }
        });
    }

    fn reset_after_call(&mut self) {
        self.local_frame_rx = None;
        self.remote_frame_rx = None;

        self.local_texture = None;
        self.remote_texture = None;
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::FullServerError => {
                self.view = View::FullServer
            }
            AppEvent::CallIncoming(peer, offer_sdp) => {
                self.view = View::CallIncoming(peer, offer_sdp);
            }
            AppEvent::CallEnded => {
                if self.controller.hang_up().is_err() {
                    self.view = View::FatalError;
                } else {
                    self.view = View::CallEnded;
                }
                self.reset_after_call();
            }
            AppEvent::Error(e) => {
                if self.controller.hang_up().is_err() {
                    self.view = View::FatalError;
                } else {
                    self.warning_msg = Some(e);
                    self.view = View::CallEnded;
                }
                self.reset_after_call();
            }
            AppEvent::FatalError => {
                let _ = self.controller.stop_media_components();
                self.view = View::FatalError;
            }
            AppEvent::CallAccepted(answer_sdp, username, peer_username) => {
                println!("Received accepted call message");
                match self.controller.get_in_call(answer_sdp) {
                    Ok((local_frame_rx, remote_frame_rx)) => {
                        self.local_frame_rx = Some(local_frame_rx);
                        self.remote_frame_rx = Some(remote_frame_rx);
                        self.view = View::Call(username, peer_username);
                    },
                    Err(e) => {
                        self.warning_msg = Some(e.to_string());
                        self.view = View::CallHub;
                    }
                }
            }
            AppEvent::CallRejected => {
                self.view = View::CallHub;
            }
            AppEvent::LocalStatsUpdate(new_stats) => {
                if let Some(current) = &mut self.last_stats {
                    current.local_sender = new_stats.local_sender;
                    current.local_receiver = new_stats.local_receiver;
                } else {
                    self.last_stats = Some(new_stats);
                }
            }
            AppEvent::RemoteStatsUpdate(new_stats) => {
                if let Some(current) = &mut self.last_stats {
                    current.remote_sender = new_stats.remote_sender;
                    current.remote_receiver = new_stats.remote_receiver;
                } else {
                    self.last_stats = Some(new_stats);
                }
            }
        }
    }
}

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
