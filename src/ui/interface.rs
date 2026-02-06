use super::views::View;
use crate::config::Config;
use crate::controller::{AppEvent, Controller};
use crate::file::file_metadata::FileMetadata;
use crate::logger::Logger;
use crate::media::frame_handler::Frame;
use crate::session::sdp::SessionDescriptionProtocol;
use crate::transport::rtcp::CallStats;
use crate::ui::GUIError as Error;
use crate::user::UserStatus;
use eframe::egui;
use eframe::epaint::{Color32, FontId};
use egui::{ColorImage, Context, RichText, TextureHandle, TextureOptions, Ui};
use rfd::FileDialog;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::thread;
use std::time::Instant;
use crate::clock::Clock;

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
            clock: Clock::new(),
        }
    }
}

impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);
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

            if let Some(rx) = &self.file_save_rx {
                if let Ok((id, metadata, option_path)) = rx.try_recv() {
                    if let Some(path) = option_path {
                        self.controller.accept_file(id, path.as_path());
                        if let Ok(mut downloads) = self.file_downloads.write() {
                            downloads.insert(id, metadata);
                        }
                        if let Ok(mut offers) = self.file_offers.write() {
                            offers.remove(&id);
                        }
                    }
                    self.file_save_rx = None;
                }
            }

            if let Some(rx) = &self.file_send_rx {
                if let Ok(result) = rx.try_recv() {
                    if let Some(path) = result {
                        if let Err(e) = self.controller.send_file(path.as_path()) {
                            self.warning_msg = Some(e.to_string());
                        }
                    }
                    self.file_send_rx = None;
                }
            }

            self.show_warning_popup(ctx);

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
                    self.show_call(username.clone(), peer_username.clone(), ui);
                }
                View::CallEnded => self.show_call_ended(ui),
                View::Error => self.show_error(ui),
                View::FatalError => self.show_fatal_error(ui),
                View::FullServer => self.show_full_server(ui),
            }

            ctx.request_repaint();
        });
    }
}

impl RoomRTCApp {
    fn show_welcome(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            let logo = egui::include_image!("assets/logo.png");
            ui.add(
                egui::Image::new(logo)
                    .max_width(300.0)
                    .maintain_aspect_ratio(true),
            );

            ui.add_space(30.0);

            ui.horizontal(|ui| {
                let button_width = 120.0;
                let spacing = ui.spacing().item_spacing.x;
                let total_width = (button_width * 2.0) + spacing;

                let x_offset = (ui.available_width() - total_width) / 2.0;
                ui.add_space(x_offset);

                let signup_text = RichText::new("Sign Up").size(18.0).strong();
                if ui
                    .add_sized(
                        [button_width, 40.0],
                        egui::Button::new(signup_text)
                            .fill(Color32::from_rgb(45, 120, 255))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    self.view = View::SignUp;
                }

                let login_text = RichText::new("Log In").size(18.0).strong();
                if ui
                    .add_sized(
                        [button_width, 40.0],
                        egui::Button::new(login_text)
                            .fill(Color32::from_rgb(45, 120, 255))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    self.view = View::LogIn;
                }
            });

            ui.add_space(20.0);

            let quit_text = RichText::new("Quit").size(18.0).strong();
            if ui
                .add_sized(
                    [120.0, 40.0],
                    egui::Button::new(quit_text)
                        .fill(Color32::from_rgb(180, 50, 50))
                        .corner_radius(8.0),
                )
                .clicked()
            {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn show_sign_up(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);

            ui.add(
                egui::Image::new(egui::include_image!("assets/logo.png"))
                    .max_width(80.0)
                    .maintain_aspect_ratio(true),
            );

            ui.add_space(10.0);

            ui.label(
                RichText::new("SIGN UP")
                    .size(32.0)
                    .strong()
                    .color(Color32::WHITE),
            );

            ui.add_space(30.0);

            let field_width = 280.0;
            let field_height = 35.0;

            let username_edit = egui::TextEdit::singleline(&mut self.username_buff)
                .hint_text(RichText::new("Username").size(16.0))
                .margin(egui::vec2(10.0, 10.0));

            ui.add_sized([field_width, field_height], username_edit);

            ui.add_space(15.0);

            let password_edit = egui::TextEdit::singleline(&mut self.password_buff)
                .password(true)
                .hint_text(RichText::new("Password").size(16.0))
                .margin(egui::vec2(10.0, 10.0));

            ui.add_sized([field_width, field_height], password_edit);

            ui.add_space(40.0);

            let btn_size = egui::vec2(210.0, 45.0);

            let can_sign_up = !self.username_buff.is_empty() && !self.password_buff.is_empty();

            let signup_btn = egui::Button::new(
                RichText::new("Sign Up")
                    .size(18.0)
                    .strong()
                    .color(Color32::WHITE),
            )
                .fill(Color32::from_rgb(0, 122, 255))
                .corner_radius(8.0);

            ui.add_enabled_ui(can_sign_up, |ui| {
                if ui.add_sized(btn_size, signup_btn).clicked() {
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
            });

            ui.add_space(15.0);

            let back_btn =
                egui::Button::new(RichText::new("Back").size(16.0).color(Color32::LIGHT_GRAY))
                    .corner_radius(8.0);

            if ui.add_sized(btn_size, back_btn).clicked() {
                self.username_buff.clear();
                self.password_buff.clear();
                self.view = View::Welcome;
            }
        });
    }

    fn show_log_in(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);

            ui.add(
                egui::Image::new(egui::include_image!("assets/logo.png"))
                    .max_width(80.0)
                    .maintain_aspect_ratio(true),
            );

            ui.add_space(10.0);

            ui.label(
                RichText::new("LOG IN")
                    .size(32.0)
                    .strong()
                    .color(Color32::WHITE),
            );

            ui.add_space(30.0);

            let field_width = 280.0;
            let field_height = 35.0;

            ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(30, 30, 30);
            ui.visuals_mut().selection.bg_fill = Color32::from_rgb(0, 122, 255);

            let username_edit = egui::TextEdit::singleline(&mut self.username_buff)
                .hint_text(RichText::new("Username").size(16.0).color(Color32::GRAY))
                .margin(egui::vec2(10.0, 10.0))
                .text_color(Color32::WHITE);

            ui.add_sized([field_width, field_height], username_edit);

            ui.add_space(15.0);

            let password_edit = egui::TextEdit::singleline(&mut self.password_buff)
                .password(true)
                .hint_text(RichText::new("Password").size(16.0).color(Color32::GRAY))
                .margin(egui::vec2(10.0, 10.0))
                .text_color(Color32::WHITE);

            ui.add_sized([field_width, field_height], password_edit);

            ui.add_space(40.0);

            let btn_size = egui::vec2(210.0, 45.0);
            let can_log_in = !self.username_buff.is_empty() && !self.password_buff.is_empty();

            let login_btn = egui::Button::new(
                RichText::new("Log In")
                    .size(18.0)
                    .strong()
                    .color(Color32::WHITE),
            )
                .fill(Color32::from_rgb(0, 122, 255))
                .corner_radius(8.0);

            ui.add_enabled_ui(can_log_in, |ui| {
                if ui.add_sized(btn_size, login_btn).clicked() {
                    let username = self.username_buff.clone();
                    let password = self.password_buff.clone();
                    match self.controller.log_in(&username, &password) {
                        Ok(()) => self.view = View::CallHub,
                        Err(e) => self.warning_msg = Some(e.to_string()),
                    }
                    self.username_buff.clear();
                    self.password_buff.clear();
                }
            });

            ui.add_space(15.0);

            let back_btn =
                egui::Button::new(RichText::new("Back").size(16.0).color(Color32::LIGHT_GRAY))
                    .corner_radius(8.0);

            if ui.add_sized(btn_size, back_btn).clicked() {
                self.username_buff.clear();
                self.password_buff.clear();
                self.view = View::Welcome;
            }
        });
    }

    fn show_call_hub(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.vertical(|ui| {
            ui.vertical_centered(|ui| {
                ui.add(
                    egui::Image::new(egui::include_image!("assets/logo.png"))
                        .max_width(150.0)
                        .maintain_aspect_ratio(true),
                );
            });

            ui.add_space(7.5);

            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 60.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_space(15.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("👤").size(22.0));
                        let username = self.controller.get_username().unwrap_or_default();
                        ui.label(
                            RichText::new(&username)
                                .strong()
                                .size(18.0)
                                .color(Color32::WHITE),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(15.0);

                        let logout_img = egui::include_image!("assets/exit_logo.png");
                        let logout_btn = egui::Button::image(
                            egui::Image::new(logout_img).fit_to_exact_size(egui::vec2(40.0, 40.0)),
                        )
                            .fill(Color32::TRANSPARENT)
                            .frame(false);

                        let btn_response = ui.add(logout_btn);

                        if btn_response
                            .on_hover_text("Log Out")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            if self.controller.log_out().is_ok() {
                                self.view = View::Welcome;
                            }
                        }
                    });
                },
            );

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

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(5.0);
                    let users_status = match self.controller.get_users_status() {
                        Ok(users_status) => users_status,
                        Err(_) => return,
                    };

                    for (username, status) in &users_status {
                        egui::Frame::new()
                            .fill(Color32::from_rgb(20, 20, 20))
                            .corner_radius(8.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let dot_color = match status {
                                        UserStatus::Available => Color32::from_rgb(50, 200, 50),
                                        UserStatus::Offline => Color32::from_rgb(60, 60, 60),
                                        UserStatus::Occupied(_) => Color32::from_rgb(200, 50, 50),
                                    };
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(12.0, 12.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(rect.center(), 6.0, dot_color);

                                    ui.add_space(5.0);

                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(username)
                                                .strong()
                                                .color(Color32::WHITE)
                                                .size(16.0),
                                        );
                                        ui.label(
                                            RichText::new(status.to_string().split(":").collect::<Vec<&str>>()[0])
                                                .size(11.0)
                                                .color(Color32::GRAY),
                                        );
                                    });

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| match status {
                                            UserStatus::Available => {
                                                let call_btn = egui::Button::new(
                                                    RichText::new("Call")
                                                        .color(Color32::WHITE)
                                                        .strong(),
                                                )
                                                    .fill(Color32::from_rgb(0, 122, 255))
                                                    .corner_radius(8.0);

                                                if ui.add_sized([70.0, 28.0], call_btn).clicked() {
                                                    if self.controller.call(username).is_ok() {
                                                        self.view = View::Calling(username.clone());
                                                    }
                                                }
                                            }
                                            UserStatus::Offline => {}
                                            UserStatus::Occupied(peer_name) => {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "In call with {}",
                                                        peer_name
                                                    ))
                                                        .size(12.0)
                                                        .italics()
                                                        .color(Color32::from_rgb(200, 200, 200)),
                                                );
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }
                });
        });
    }

    fn show_calling(&mut self, peer_username: String, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);

            let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 150.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 75.0, Color32::from_rgb(30, 30, 30));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "👤",
                FontId::proportional(80.0),
                Color32::LIGHT_GRAY,
            );

            ui.add_space(30.0);

            ui.label(
                RichText::new(&peer_username)
                    .size(40.0)
                    .strong()
                    .color(Color32::WHITE),
            );

            ui.add_space(10.0);

            ui.label(
                RichText::new("Calling...")
                    .size(20.0)
                    .color(Color32::from_rgb(0, 122, 255)),
            );

            ui.add_space(80.0);
        });
    }

    fn show_call_incoming(
        &mut self,
        peer_username: String,
        sdp_offer: SessionDescriptionProtocol,
        ctx: &Context,
        ui: &mut Ui,
    ) {
        if let Some(start_time) = self.time_call_incoming {
            let diff = start_time.elapsed().as_secs();
            if diff >= 15 {
                self.warning_msg = Some("Time to accept call expired".to_string());
                match self.controller.reject_call(peer_username.clone()) {
                    Ok(()) => {}
                    Err(e) => self.warning_msg = Some(e.to_string()),
                }
                self.time_call_incoming = None;
                self.view = View::CallHub;
                return;
            }
        } else {
            self.time_call_incoming = Some(Instant::now());
        };
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);

            let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 150.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 75.0, Color32::from_rgb(30, 30, 30));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "👤",
                FontId::proportional(80.0),
                Color32::LIGHT_GRAY,
            );

            ui.add_space(30.0);

            ui.label(
                RichText::new(&peer_username)
                    .size(40.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.label(
                RichText::new("Incoming call...")
                    .size(20.0)
                    .color(Color32::from_rgb(0, 122, 255)),
            );

            ui.add_space(60.0);

            ui.horizontal(|ui| {
                let buttons_width = (130.0 * 2.0) + 40.0;
                let start_space = (ui.available_width() - buttons_width) / 2.0;
                ui.add_space(start_space);

                let accept_btn = egui::Button::new(
                    RichText::new("Accept")
                        .size(18.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                    .fill(Color32::from_rgb(50, 180, 50))
                    .corner_radius(30.0);

                if ui.add_sized([130.0, 60.0], accept_btn).clicked() {
                    match self
                        .controller
                        .accept_call(peer_username.clone(), &sdp_offer)
                    {
                        Ok((local_frame_rx, remote_frame_rx)) => {
                            if let Ok(username) = self.controller.get_username() {
                                self.local_frame_rx = Some(local_frame_rx);
                                self.remote_frame_rx = Some(remote_frame_rx);
                                self.view = View::Call(username, peer_username.clone());
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
                            }
                        }
                        Err(e) => {
                            self.warning_msg = Some(e.to_string());
                            self.view = View::CallHub;
                        }
                    }
                }

                ui.add_space(40.0);

                let decline_btn = egui::Button::new(
                    RichText::new("Decline")
                        .size(18.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                    .fill(Color32::from_rgb(220, 50, 50))
                    .corner_radius(30.0);

                if ui.add_sized([130.0, 60.0], decline_btn).clicked() {
                    match self.controller.reject_call(peer_username.clone()) {
                        Ok(()) => {}
                        Err(e) => self.warning_msg = Some(e.to_string()),
                    }
                    self.view = View::CallHub;
                    self.time_call_incoming = None;
                }
            });

            ui.add_space(40.0);
        });
    }

    fn show_call(&mut self, username: String, peer_username: String, ctx: &Context, ui: &mut Ui) {
        self.show_side_panel(ctx);
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

        ui.vertical_centered(|ui| {
            ui.add_space(10.0);

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
                        .show(ui, |ui| {
                            self.show_local_camera(ui);
                        });
                });

                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&peer_username)
                            .size(14.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.add_space(5.0);

                    egui::Frame::canvas(ui.style())
                        .fill(Color32::BLACK)
                        .show(ui, |ui| {
                            self.show_remote_camera(ui);
                        });
                });
            });

            ui.add_space(25.0);

            let exit_btn =
                egui::Button::new(RichText::new("End Call").strong().color(Color32::WHITE))
                    .fill(Color32::from_rgb(200, 40, 40))
                    .corner_radius(20.0);
            if ui.add_sized([180.0, 45.0], exit_btn).clicked() {
                if self.controller.hang_up().is_err() {
                    self.view = View::FatalError;
                } else {
                    self.reset_after_call();
                    self.view = View::CallHub;
                }
            }

            let file_btn =
                egui::Button::new(RichText::new("Send file").strong().color(Color32::WHITE))
                    .fill(Color32::ORANGE)
                    .corner_radius(20.0);
            if ui.add_sized([180.0, 45.0], file_btn).clicked() {
                self.file_send_rx = Some(pick_file_to_send());
            }

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);

            if let Some(stats) = &self.last_stats {
                egui::Frame::new()
                    .fill(Color32::from_rgb(25, 25, 25))
                    .corner_radius(10.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_max_width(400.0);

                        ui.label(
                            RichText::new("📊 NETWORK DIAGNOSTICS")
                                .size(12.0)
                                .strong()
                                .color(Color32::from_rgb(0, 150, 255)),
                        );
                        ui.add_space(8.0);

                        egui::Grid::new("stats_grid")
                            .num_columns(2)
                            .spacing([40.0, 8.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("Jitter:").color(Color32::GRAY));
                                ui.label(format!("{} ms", stats.remote_receiver.jitter));
                                ui.end_row();

                                ui.label(RichText::new("Packets Lost:").color(Color32::GRAY));
                                ui.label(
                                    RichText::new(format!(
                                        "{}",
                                        stats.remote_receiver.packets_lost
                                    ))
                                        .color(
                                            if stats.remote_receiver.packets_lost > 0 {
                                                Color32::KHAKI
                                            } else {
                                                Color32::GREEN
                                            },
                                        ),
                                );
                                ui.end_row();

                                ui.label(RichText::new("Sent / Received:").color(Color32::GRAY));
                                ui.label(format!(
                                    "{} / {}",
                                    stats.local_sender.packets_sent,
                                    stats.local_receiver.packets_received
                                ));
                                ui.end_row();
                            });
                    });
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

            let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 150.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 75.0, Color32::from_rgb(30, 30, 30));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "📵",
                FontId::proportional(80.0),
                Color32::from_rgb(150, 150, 150),
            );

            ui.add_space(30.0);

            ui.label(
                RichText::new("Call Ended")
                    .size(40.0)
                    .strong()
                    .color(Color32::WHITE),
            );

            ui.add_space(10.0);

            ui.label(
                RichText::new("The other user has disconnected.")
                    .size(18.0)
                    .color(Color32::from_rgb(140, 140, 140)),
            );

            ui.add_space(80.0);

            let back_btn = egui::Button::new(
                RichText::new("Back to hub")
                    .size(18.0)
                    .strong()
                    .color(Color32::WHITE),
            )
                .fill(Color32::from_rgb(0, 122, 255))
                .corner_radius(30.0);

            if ui
                .add_sized([240.0, 60.0], back_btn)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.view = View::CallHub;
            }

            ui.add_space(40.0);
        });
    }

    fn show_full_server(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);

            let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 150.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 75.0, Color32::from_rgb(40, 20, 20));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "🚫",
                FontId::proportional(80.0),
                Color32::from_rgb(220, 50, 50),
            );

            ui.add_space(30.0);

            ui.label(
                RichText::new("Server Full")
                    .size(40.0)
                    .strong()
                    .color(Color32::WHITE),
            );

            ui.add_space(10.0);

            ui.label(
                RichText::new("Maximum capacity reached.\nPlease try again in a few minutes.")
                    .size(18.0)
                    .color(Color32::from_rgb(160, 160, 160)),
            );

            ui.add_space(80.0);

            let close_btn = egui::Button::new(
                RichText::new("Close Application")
                    .size(18.0)
                    .strong()
                    .color(Color32::WHITE),
            )
                .fill(Color32::from_rgb(60, 60, 60))
                .corner_radius(30.0);

            if ui
                .add_sized([240.0, 60.0], close_btn)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }

            ui.add_space(40.0);
        });
    }

    fn update_video_textures(&mut self, ctx: &Context) -> Result<(), Error> {
        if let Some(local_rx) = self.local_frame_rx.take() {
            spawn_texture_thread(
                local_rx,
                ctx.clone(),
                self.local_texture.clone(),
                "local_camera".to_string()
            );
        }

        if let Some(remote_rx) = self.remote_frame_rx.take() {
            spawn_texture_thread(
                remote_rx,
                ctx.clone(),
                self.remote_texture.clone(),
                "remote_camera".to_string()
            );
        }

        Ok(())
    }

    fn show_local_camera(&self, ui: &mut Ui) -> Result<(), Error> {
        let guard = self.local_texture.lock()
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
        let guard = self.remote_texture.lock()
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

            let error_text = self
                .error_msg
                .as_deref()
                .unwrap_or("An unknown error occurred");
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
            ui.add_space(80.0);

            let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 150.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 75.0, Color32::from_rgb(60, 20, 20));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "⚠",
                FontId::proportional(80.0),
                Color32::WHITE,
            );

            ui.add_space(30.0);

            ui.label(
                RichText::new("Fatal Error")
                    .size(40.0)
                    .strong()
                    .color(Color32::WHITE),
            );

            ui.add_space(10.0);

            ui.label(
                RichText::new("An unexpected error occurred and the\napplication needs to close.")
                    .size(18.0)
                    .color(Color32::from_rgb(180, 150, 150)),
            );

            ui.add_space(80.0);

            let close_btn = egui::Button::new(
                RichText::new("Exit Application")
                    .size(18.0)
                    .strong()
                    .color(Color32::WHITE),
            )
                .fill(Color32::from_rgb(180, 40, 40))
                .corner_radius(30.0);

            if ui
                .add_sized([240.0, 60.0], close_btn)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                std::process::exit(0);
            }

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
    }

    fn handle_event(
        &mut self,
        event: AppEvent,
        ctx: &Context,
    ) {
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
                            return;
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
            AppEvent::RemoteFileOffer(id, metadata) => match self.file_offers.write() {
                Ok(mut f) => {
                    f.insert(id, metadata);
                }
                Err(_) => return,
            },
            AppEvent::FileDownloadCompleted(id) => match self.file_downloads.write() {
                Ok(mut f) => { f.remove(&id); },
                Err(_) => {
                    println!("failed to remove file from downloading");
                    return
                },
            }
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

                    let mut id_to_remove: Option<u32> = None;
                    if let Ok(offers) = self.file_offers.read() {
                        if offers.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("No pending files")
                                        .italics()
                                        .color(Color32::GRAY),
                                );
                            });
                        }

                        if let Ok(downloads) = self.file_downloads.read() {
                            let dots_ammount = ((self.clock.now()/1000) % 4) as usize;
                            let dots = ".".repeat(dots_ammount);

                            for (_id, metadata) in downloads.iter() {
                                egui::Frame::group(ui.style())
                                    .fill(Color32::from_rgb(35, 35, 35))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.label(RichText::new(&metadata.name).strong());
                                        ui.label(format!("{} B", metadata.size));
                                        ui.label(format!("Downloading{dots}"));
                                    });
                                ui.add_space(8.0);
                            }
                        }

                        for (id, offer) in offers.iter() {
                            egui::Frame::group(ui.style())
                                .fill(Color32::from_rgb(35, 35, 35))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.label(RichText::new(&offer.name).strong());
                                    ui.label(format!("{} B", offer.size));

                                    ui.horizontal(|ui| {
                                        let accept_btn = egui::Button::new(
                                            RichText::new("✔").color(Color32::GREEN),
                                        );
                                        if ui.add(accept_btn).on_hover_text("Accept file").clicked()
                                        {
                                            self.file_save_rx = Some(select_path_to_save_file(*id, offer.clone()));
                                        }

                                        let reject_btn = egui::Button::new(
                                            RichText::new("✖").color(Color32::RED),
                                        );
                                        if ui.add(reject_btn).on_hover_text("Reject file").clicked()
                                        {
                                            self.controller.reject_file(*id);
                                            id_to_remove = Some(*id);
                                        }
                                    });
                                });
                            ui.add_space(8.0);
                        }
                    }
                    if let Some(id) = id_to_remove && self.file_save_rx.is_none(){
                        self.remove_file_from_offers(id)
                    }
                });
            });
    }
    fn remove_file_from_offers(&mut self, id: u32) {
        match self.file_offers.write() {
            Ok(mut f) => {
                f.remove(&id);
            }
            Err(_) => return,
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

fn select_path_to_save_file(id: u32, metadata: FileMetadata) -> Receiver<(u32, FileMetadata, Option<PathBuf>)> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let path = FileDialog::new()
            .set_file_name(&metadata.name)
            .save_file();

        if let Err(_) = tx.send((id, metadata, path)) {
            return;
        }
    });

    rx
}

fn pick_file_to_send() -> Receiver<Option<PathBuf>> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let path = FileDialog::new()
            .pick_file();

        if let Err(_) = tx.send(path) {
            return;
        }
    });

    rx
}
