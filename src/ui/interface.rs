use super::Widget;
use super::views::View;
use crate::clock::Clock;
use crate::config::Config;
use crate::controller::{AppEvent, Controller};
use crate::file::file_metadata::FileMetadata;
use crate::logger::Logger;
use crate::media::frame_handler::Frame;
use crate::session::sdp::SessionDescriptionProtocol;
use crate::transport::rtcp::CallStats;
use crate::ui::GUIError as Error;
use eframe::egui;
use eframe::epaint::{Color32, FontId};
use egui::{ColorImage, Context, RichText, TextureHandle, TextureOptions, Ui};
use rfd::FileDialog;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::thread;
use std::time::Instant;

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
    local_texture: Arc<Mutex<Option<TextureHandle>>>,
    remote_texture: Arc<Mutex<Option<TextureHandle>>>,

    // Error handling
    error_msg: Option<String>,
    warning_msg: Option<String>,

    last_stats: Option<CallStats>,
    time_call_incoming: Option<Instant>,

    // File
    file_offers: Arc<RwLock<HashMap<u32, FileMetadata>>>,
    file_downloads: Arc<RwLock<HashMap<u32, FileMetadata>>>,
    file_send_rx: Option<Receiver<Option<PathBuf>>>,
    file_save_rx: Option<Receiver<(u32, FileMetadata, Option<PathBuf>)>>,

    // Mic
    is_muted: bool,

    //Aux
    clock: Clock,
}

impl RoomRTCApp {
    /// Create a new `RoomRTCApp` from the given configuration.
    ///
    /// Inputs:
    /// - `config`: application configuration used to construct the
    ///   `Controller` and to configure the session behavior.
    ///
    /// Outputs:
    /// - A fully initialized `RoomRTCApp` with channels created for
    ///   receiving local frames, remote frames and events. The
    ///   `Controller` is created and returned in a running state.
    #[must_use]
    pub fn new(config: Config, server_address: SocketAddr, logger: Logger) -> Self {
        let config = Arc::new(config);
        let (event_tx, event_rx) = mpsc::channel();

        let controller =
            match init_controller(event_tx, config.clone(), server_address, logger.clone()) {
                Ok(c) => c,
                Err(_) => std::process::exit(1),
            };

        Self {
            view: View::default(),
            controller,
            event_rx,
            local_frame_rx: None,
            remote_frame_rx: None,
            username_buff: String::new(),
            password_buff: String::new(),
            local_texture: Arc::new(Mutex::new(None)),
            remote_texture: Arc::new(Mutex::new(None)),
            error_msg: None,
            warning_msg: None,
            last_stats: None,
            time_call_incoming: None,
            file_offers: Arc::new(RwLock::new(HashMap::new())),
            file_downloads: Arc::new(RwLock::new(HashMap::new())),
            file_send_rx: None,
            file_save_rx: None,
            is_muted: false,
            clock: Clock::new(),
        }
    }
}

impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);

            self.run_app_event_loop(ctx);
            self.manage_files();
            self.show_warning_popup(ctx);
            self.render_current_view(ctx, ui);

            ctx.request_repaint();
        });
    }
}

impl RoomRTCApp {
    fn show_welcome(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);

            ui.logo(300.0);
            ui.add_space(30.0);

            ui.centered_and_sized_buttons(120.0, 2, |ui| {
                if ui
                    .primary_button("Sign Up", egui::vec2(120.0, 40.0))
                    .clicked()
                {
                    self.view = View::SignUp;
                }

                if ui
                    .primary_button("Log In", egui::vec2(120.0, 40.0))
                    .clicked()
                {
                    self.view = View::LogIn;
                }
            });

            ui.add_space(20.0);

            if ui.danger_button("Quit", egui::vec2(120.0, 40.0)).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn show_sign_up(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.logo(80.0);

            ui.add_space(10.0);
            ui.title("SIGN UP");

            ui.add_space(30.0);
            ui.text_input(&mut self.username_buff, "Username", false);
            ui.add_space(15.0);
            ui.text_input(&mut self.password_buff, "Password", true);

            ui.add_space(40.0);

            let can_sign_up = !self.username_buff.is_empty() && !self.password_buff.is_empty();

            ui.centered_and_sized_buttons(120.0, 2, |ui| {
                ui.add_enabled_ui(can_sign_up, |ui| {
                    if ui
                        .primary_button("Sign Up", egui::vec2(120.0, 40.0))
                        .clicked()
                    {
                        self.handle_signup_action();
                    }
                });

                if ui.neutral_button("Back", egui::vec2(120.0, 40.0)).clicked() {
                    self.clear_auth_buffers();
                    self.view = View::Welcome;
                }
            });
        });
    }

    fn show_log_in(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.logo(80.0);

            ui.add_space(10.0);
            ui.title("LOG IN");

            ui.add_space(30.0);
            ui.text_input(&mut self.username_buff, "Username", false);
            ui.add_space(15.0);
            ui.text_input(&mut self.password_buff, "Password", true);

            ui.add_space(40.0);

            let can_log_in = !self.username_buff.is_empty() && !self.password_buff.is_empty();

            ui.centered_and_sized_buttons(120.0, 2, |ui| {
                ui.add_enabled_ui(can_log_in, |ui| {
                    if ui
                        .primary_button("Log In", egui::vec2(120.0, 40.0))
                        .clicked()
                    {
                        self.handle_login_action();
                    }
                });

                if ui.neutral_button("Back", egui::vec2(120.0, 40.0)).clicked() {
                    self.clear_auth_buffers();
                    self.view = View::Welcome;
                }
            });
        });
    }

    fn show_call_hub(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.vertical(|ui| {
            ui.vertical_centered(|ui| ui.logo(150.0));
            ui.add_space(7.5);

            let username = match self.controller.get_username() {
                Ok(u) => u,
                Err(_) => String::new(),
            };
            ui.user_profile_header(&username, || {
                if self.controller.log_out().is_ok() {
                    self.view = View::Welcome;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(
                RichText::new(" CONTACTS")
                    .color(Color32::GRAY)
                    .size(13.0)
                    .strong(),
            );
            ui.add_space(15.0);

            self.render_contacts_list(ui);
        });
    }

    fn show_calling(&mut self, peer_username: String, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);

            ui.big_avatar("👤", 150.0);

            ui.add_space(30.0);

            ui.label(
                RichText::new(&peer_username)
                    .size(40.0)
                    .strong()
                    .color(Color32::WHITE),
            );

            ui.add_space(10.0);

            ui.status_label("Calling...", Color32::from_rgb(0, 122, 255));

            ui.add_space(80.0);
        });
    }

    fn show_call_incoming(
        &mut self,
        peer: String,
        sdp: SessionDescriptionProtocol,
        ctx: &Context,
        ui: &mut Ui,
    ) {
        if self.call_timeout(&peer) {
            return;
        }

        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.big_avatar("👤", 150.0);
            ui.add_space(30.0);
            ui.label(
                RichText::new(&peer)
                    .size(40.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.status_label("Incoming call...", Color32::from_rgb(0, 122, 255));
            ui.add_space(60.0);

            ui.centered_and_sized_buttons(130.0, 2, |ui| {
                if ui
                    .call_button("Accept", Color32::from_rgb(50, 180, 50))
                    .clicked()
                {
                    self.handle_accept_logic(peer.clone(), sdp, ctx);
                }
                ui.add_space(32.0);

                if ui
                    .call_button("Decline", Color32::from_rgb(220, 50, 50))
                    .clicked()
                {
                    self.handle_decline_logic(peer);
                }
            });
        });
    }

    fn show_call(&mut self, username: String, peer_username: String, ctx: &Context, ui: &mut Ui) {
        self.show_side_panel(ctx);
        if !self.handle_call_video_update(ctx) {
            return;
        }

        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            self.show_video_layout(ui, &username, &peer_username);
            ui.add_space(25.0);

            self.show_bottom_toolbar(ui);

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            if let Some(stats) = &self.last_stats {
                ui.network_stats_panel(stats);
            } else {
                ui.add_space(10.0);
                ui.spinner();
                ui.label(
                    RichText::new("Waiting for RTCP reports...")
                        .italics()
                        .color(Color32::GRAY),
                );
            }
        });
    }

    fn show_call_ended(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);

            ui.big_avatar("📵", 150.0);

            ui.add_space(30.0);

            ui.title("Call Ended");
            ui.add_space(10.0);

            ui.label(
                RichText::new("The other user has disconnected.")
                    .size(18.0)
                    .color(Color32::from_rgb(140, 140, 140)),
            );

            ui.add_space(80.0);

            ui.centered_and_sized_buttons(240.0, 1, |ui| {
                if ui
                    .primary_button("Back to hub", egui::vec2(240.0, 60.0))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.view = View::CallHub;
                }
            });

            ui.add_space(40.0);
        });
    }

    fn show_full_server(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.big_avatar("🚫", 150.0);
            ui.add_space(30.0);

            ui.title("Server Full");
            ui.add_space(10.0);
            ui.label(
                RichText::new("Maximum capacity reached.\nPlease try again in a few minutes.")
                    .size(18.0)
                    .color(Color32::from_rgb(160, 160, 160)),
            );

            ui.add_space(80.0);

            ui.centered_and_sized_buttons(240.0, 1, |ui| {
                if ui
                    .neutral_button("Close Application", egui::vec2(240.0, 60.0))
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.add_space(40.0);
        });
    }

    fn update_video_textures(&mut self, ctx: &Context) -> Result<(), Error> {
        if let Some(local_rx) = self.local_frame_rx.take() {
            spawn_texture_thread(
                local_rx,
                ctx.clone(),
                self.local_texture.clone(),
                "local_camera".to_string(),
            );
        }

        if let Some(remote_rx) = self.remote_frame_rx.take() {
            spawn_texture_thread(
                remote_rx,
                ctx.clone(),
                self.remote_texture.clone(),
                "remote_camera".to_string(),
            );
        }

        Ok(())
    }

    fn show_local_camera(&self, ui: &mut Ui) -> Result<(), Error> {
        let guard = self
            .local_texture
            .lock()
            .map_err(|e| Error::MapError(e.to_string()))?;
        if let Some(texture) = guard.as_ref() {
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
        Ok(())
    }

    fn show_remote_camera(&self, ui: &mut Ui) -> Result<(), Error> {
        let guard = self
            .remote_texture
            .lock()
            .map_err(|e| Error::MapError(e.to_string()))?;
        if let Some(texture) = guard.as_ref() {
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
        Ok(())
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

            let error_text = match self.error_msg.as_deref() {
                Some(s) => s,
                None => "An unknown error occurred",
            };
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

    // fn show_fatal_error(&mut self, ui: &mut Ui) {
    //     ui.vertical_centered(|ui| {
    //         ui.add_space(80.0);
    //
    //         let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 150.0), egui::Sense::hover());
    //         ui.painter()
    //             .circle_filled(rect.center(), 75.0, Color32::from_rgb(60, 20, 20));
    //         ui.painter().text(
    //             rect.center(),
    //             egui::Align2::CENTER_CENTER,
    //             "⚠",
    //             FontId::proportional(80.0),
    //             Color32::WHITE,
    //         );
    //
    //         ui.add_space(30.0);
    //
    //         ui.label(
    //             RichText::new("Fatal Error")
    //                 .size(40.0)
    //                 .strong()
    //                 .color(Color32::WHITE),
    //         );
    //
    //         ui.add_space(10.0);
    //
    //         ui.label(
    //             RichText::new("An unexpected error occurred and the\napplication needs to close.")
    //                 .size(18.0)
    //                 .color(Color32::from_rgb(180, 150, 150)),
    //         );
    //
    //         ui.add_space(80.0);
    //
    //         let close_btn = egui::Button::new(
    //             RichText::new("Exit Application")
    //                 .size(18.0)
    //                 .strong()
    //                 .color(Color32::WHITE),
    //         )
    //             .fill(Color32::from_rgb(180, 40, 40))
    //             .corner_radius(30.0);
    //
    //         if ui
    //             .add_sized([240.0, 60.0], close_btn)
    //             .on_hover_cursor(egui::CursorIcon::PointingHand)
    //             .clicked()
    //         {
    //             std::process::exit(0);
    //         }
    //
    //         ui.add_space(40.0);
    //     });
    // }

    fn show_fatal_error(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);

            ui.big_avatar("⚠", 150.0);

            ui.add_space(30.0);
            ui.title("Fatal Error");
            ui.add_space(10.0);

            ui.label(
                RichText::new("An unexpected error occurred and the\napplication needs to close.")
                    .size(18.0)
                    .color(Color32::from_rgb(180, 150, 150)),
            );

            ui.add_space(80.0);

            ui.centered_and_sized_buttons(240.0, 1, |ui| {
                if ui
                    .danger_button("Exit Application", egui::vec2(240.0, 60.0))
                    .clicked()
                {
                    std::process::exit(0);
                }
            });

            ui.add_space(40.0);
        });
    }

    fn reset_after_call(&mut self) {
        self.local_frame_rx = None;
        self.remote_frame_rx = None;

        self.local_texture = Arc::new(Mutex::new(None));
        self.remote_texture = Arc::new(Mutex::new(None));

        self.time_call_incoming = None;
        self.file_offers = Arc::new(RwLock::new(HashMap::new()));
        self.file_downloads = Arc::new(RwLock::new(HashMap::new()));

        self.is_muted = false;
    }

    fn handle_event(&mut self, event: AppEvent, ctx: &Context) {
        match event {
            AppEvent::FullServerError => self.view = View::FullServer,
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
                match self.controller.get_in_call(answer_sdp) {
                    Ok((local_frame_rx, remote_frame_rx)) => {
                        self.local_frame_rx = Some(local_frame_rx);
                        self.remote_frame_rx = Some(remote_frame_rx);
                        self.view = View::Call(username, peer_username);
                        if let Err(e) = self.update_video_textures(ctx) {
                            self.warning_msg = Some(e.to_string());
                            if self.controller.hang_up().is_err() {
                                self.view = View::FatalError;
                            } else {
                                self.reset_after_call();
                                self.view = View::CallHub;
                            }
                        }
                    }
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
            AppEvent::RemoteFileOffer(id, metadata) => {
                if let Ok(mut f) = self.file_offers.write() {
                    f.insert(id, metadata);
                }
            }
            AppEvent::FileDownloadCompleted(id) => match self.file_downloads.write() {
                Ok(mut f) => {
                    f.remove(&id);
                }
                Err(_) => {
                    println!("failed to remove file from downloading");
                }
            },
        }
    }

    fn show_side_panel(&mut self, ctx: &Context) {
        egui::SidePanel::right("files_panel")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("FILES")
                            .heading()
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.separator();
                });

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(5.0);

                    self.show_downloading_files(ui);
                    let id_to_remove = self.show_file_offers(ui);

                    if let Some(id) = id_to_remove
                        && self.file_save_rx.is_none()
                    {
                        self.remove_file_from_offers(id);
                    }
                });
            });
    }

    fn remove_file_from_offers(&mut self, id: u32) {
        if let Ok(mut f) = self.file_offers.write() {
            f.remove(&id);
        }
    }

    fn run_app_event_loop(&mut self, ctx: &Context) {
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => self.handle_event(event, ctx),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.view = View::FatalError;
                    break;
                }
            }
        }
    }

    fn manage_files(&mut self) {
        if let Some(rx) = &self.file_save_rx
            && let Ok((id, metadata, option_path)) = rx.try_recv()
        {
            if let Some(path) = option_path {
                if let Err(e) = self.controller.accept_file(id, path.as_path()) {
                    self.warning_msg = Some(format!("Failed to accept file: {e}"));
                }
                if let Ok(mut downloads) = self.file_downloads.write() {
                    downloads.insert(id, metadata);
                }
                if let Ok(mut offers) = self.file_offers.write() {
                    offers.remove(&id);
                }
            }
            self.file_save_rx = None;
        }

        if let Some(rx) = &self.file_send_rx
            && let Ok(Some(path)) = rx.try_recv()
        {
            if let Err(e) = self.controller.send_file(path.as_path()) {
                self.warning_msg = Some(e.to_string());
            }
            self.file_send_rx = None;
        }
    }

    fn render_current_view(&mut self, ctx: &Context, ui: &mut Ui) {
        match &self.view {
            View::Welcome => self.show_welcome(ui),
            View::SignUp => self.show_sign_up(ui),
            View::LogIn => self.show_log_in(ui),
            View::Calling(peer_username) => self.show_calling(peer_username.clone(), ui),
            View::CallIncoming(peer_username, sdp_offer) => {
                self.show_call_incoming(peer_username.clone(), sdp_offer.clone(), ctx, ui);
            }
            View::CallHub => self.show_call_hub(ui),
            View::Call(username, peer_username) => {
                self.show_call(username.clone(), peer_username.clone(), ctx, ui);
            }
            View::CallEnded => self.show_call_ended(ui),
            View::Error => self.show_error(ui),
            View::FatalError => self.show_fatal_error(ui),
            View::FullServer => self.show_full_server(ui),
        }
    }

    fn handle_signup_action(&mut self) {
        let username = self.username_buff.clone();
        let password = self.password_buff.clone();

        match self.controller.sign_up(username, password) {
            Ok(()) => self.view = View::LogIn,
            Err(e) => self.warning_msg = Some(e.to_string()),
        }
        self.clear_auth_buffers();
    }

    fn clear_auth_buffers(&mut self) {
        self.username_buff.clear();
        self.password_buff.clear();
    }

    fn handle_login_action(&mut self) {
        let username = self.username_buff.clone();
        let password = self.password_buff.clone();

        match self.controller.log_in(&username, &password) {
            Ok(()) => self.view = View::CallHub,
            Err(e) => self.warning_msg = Some(e.to_string()),
        }
        self.clear_auth_buffers();
    }

    fn render_contacts_list(&mut self, ui: &mut Ui) {
        let Ok(users_status) = self.controller.get_users_status() else {
            return;
        };

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.add_space(5.0);
                for (username, status) in users_status {
                    let u_name = username.clone();
                    ui.contact_card(&username, &status, || {
                        if self.controller.call(&u_name).is_ok() {
                            self.view = View::Calling(u_name);
                        }
                    });
                    ui.add_space(4.0);
                }
            });
    }

    fn handle_accept_logic(
        &mut self,
        peer: String,
        sdp: SessionDescriptionProtocol,
        ctx: &Context,
    ) {
        match self.controller.accept_call(peer.clone(), &sdp) {
            Ok((local, remote)) => {
                if let Ok(username) = self.controller.get_username() {
                    self.local_frame_rx = Some(local);
                    self.remote_frame_rx = Some(remote);
                    self.view = View::Call(username, peer);
                    if let Err(e) = self.update_video_textures(ctx) {
                        self.warning_msg = Some(e.to_string());
                        if self.controller.hang_up().is_err() {
                            self.view = View::FatalError;
                        } else {
                            self.reset_after_call();
                            self.view = View::CallHub;
                        }
                    }
                }
            }
            Err(e) => {
                self.warning_msg = Some(e.to_string());
                self.view = View::CallHub;
            }
        }
    }

    fn handle_decline_logic(&mut self, peer: String) {
        match self.controller.reject_call(peer) {
            Ok(()) => {}
            Err(e) => self.warning_msg = Some(e.to_string()),
        }
        self.view = View::CallHub;
        self.time_call_incoming = None;
    }

    fn call_timeout(&mut self, peer: &str) -> bool {
        let start = *self.time_call_incoming.get_or_insert_with(Instant::now);
        if start.elapsed().as_secs() >= 15 {
            self.warning_msg = Some("Time to accept call expired".to_string());
            let _ = self.controller.reject_call(peer.to_string());
            self.time_call_incoming = None;
            self.view = View::CallHub;
            return true;
        }
        false
    }

    fn handle_call_video_update(&mut self, ctx: &Context) -> bool {
        if let Err(e) = self.update_video_textures(ctx) {
            self.warning_msg = Some(e.to_string());
            if self.controller.hang_up().is_err() {
                self.view = View::FatalError;
            } else {
                self.reset_after_call();
                self.view = View::CallHub;
            }
            return false;
        }
        true
    }

    fn show_video_layout(&mut self, ui: &mut Ui, username: &str, peer: &str) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("YOU ({})", username))
                        .size(14.0)
                        .color(Color32::GRAY),
                );
                ui.add_space(5.0);
                egui::Frame::canvas(ui.style())
                    .fill(Color32::BLACK)
                    .show(ui, |ui| self.show_local_camera(ui));
            });
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(peer)
                        .size(14.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(5.0);
                egui::Frame::canvas(ui.style())
                    .fill(Color32::BLACK)
                    .show(ui, |ui| self.show_remote_camera(ui));
            });
        });
    }

    fn show_bottom_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let (text, color) = if self.is_muted {
                ("🔇 Unmute", Color32::from_rgb(200, 100, 0))
            } else {
                ("🎤 Mute", Color32::from_rgb(60, 60, 60))
            };
            if ui.call_action_btn(text, color, 150.0).clicked() {
                self.controller.toggle_audio();
                self.is_muted = !self.is_muted;
            }
            ui.add_space(20.0);
            if ui
                .call_action_btn("End Call", Color32::from_rgb(200, 40, 40), 180.0)
                .clicked()
            {
                if self.controller.hang_up().is_err() {
                    self.view = View::FatalError;
                } else {
                    self.reset_after_call();
                    self.view = View::CallHub;
                }
            }
            ui.add_space(20.0);
            if ui
                .call_action_btn("Send file", Color32::ORANGE, 180.0)
                .clicked()
            {
                self.file_send_rx = Some(pick_file_to_send());
            }
        });
    }

    fn show_downloading_files(&mut self, ui: &mut Ui) {
        if let Ok(downloads) = self.file_downloads.read() {
            let dots = ".".repeat(((self.clock.now() / 1000) % 4) as usize);
            for (_, meta) in downloads.iter() {
                ui.file_card(&meta.name, meta.size, |ui| {
                    ui.label(
                        RichText::new(format!("Downloading{dots}"))
                            .italics()
                            .color(Color32::LIGHT_BLUE),
                    );
                });
            }
        }
    }

    fn show_file_offers(&mut self, ui: &mut Ui) -> Option<u32> {
        let mut to_remove = None;
        if let Ok(offers) = self.file_offers.read() {
            if offers.is_empty() && self.file_downloads.read().map_or(true, |d| d.is_empty()) {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("No pending files")
                            .italics()
                            .color(Color32::GRAY),
                    )
                });
            }
            for (id, offer) in offers.iter() {
                ui.file_card(&offer.name, offer.size, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("✔").color(Color32::GREEN))
                            .on_hover_text("Accept")
                            .clicked()
                        {
                            self.file_save_rx = Some(select_path_to_save_file(*id, offer.clone()));
                        }
                        if ui
                            .button(RichText::new("✖").color(Color32::RED))
                            .on_hover_text("Reject")
                            .clicked()
                        {
                            if let Err(e) = self.controller.reject_file(*id) {
                                self.warning_msg = Some(format!("Failed to reject file: {e}"));
                            }
                            to_remove = Some(*id);
                        }
                    });
                });
            }
        }
        to_remove
    }
}

fn select_path_to_save_file(
    id: u32,
    metadata: FileMetadata,
) -> Receiver<(u32, FileMetadata, Option<PathBuf>)> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let path = FileDialog::new().set_file_name(&metadata.name).save_file();

        let _ = tx.send((id, metadata, path));
    });

    rx
}

fn pick_file_to_send() -> Receiver<Option<PathBuf>> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let path = FileDialog::new().pick_file();

        let _ = tx.send(path);
    });

    rx
}

fn spawn_texture_thread(
    rx: Receiver<Frame>,
    ctx: Context,
    texture_arc: Arc<Mutex<Option<TextureHandle>>>,
    texture_name: String,
) {
    thread::spawn(move || {
        while let Ok(frame) = rx.recv() {
            let mut last_frame = frame;
            while let Ok(more_recent) = rx.try_recv() {
                last_frame = more_recent;
            }
            let color_img =
                ColorImage::from_rgb([last_frame.width, last_frame.height], &last_frame.data);

            let mut guard = match texture_arc.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };

            if let Some(texture) = guard.as_mut() {
                texture.set(color_img, TextureOptions::default());
            } else {
                *guard =
                    Some(ctx.load_texture(&texture_name, color_img, TextureOptions::default()));
            }

            ctx.request_repaint();
        }
    });
}

fn init_controller(
    event_tx: Sender<AppEvent>,
    config: Arc<Config>,
    server_address: SocketAddr,
    logger: Logger,
) -> Result<Controller, Error> {
    let mut controller = match Controller::new(
        event_tx,
        &config,
        server_address,
        logger.context("Controller"),
    ) {
        Ok(c) => c,
        Err(e) => Err(Error::MapError(e.to_string()))?,
    };

    controller
        .initial_handshake()
        .map_err(|e| Error::MapError(e.to_string()))?;
    Ok(controller)
}
