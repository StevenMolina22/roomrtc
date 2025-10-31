use std::sync::{mpsc};
use std::sync::mpsc::Receiver;
use eframe::egui;
use egui::{ColorImage, Context, TextureHandle, TextureOptions, Ui};
use super::views::View;
use crate::{controller::Controller};
use crate::frame_handler::Frame;

pub struct RoomRTCApp {
    view: View,
    controller: Controller,
    rx_local: Receiver<Frame>,
    rx_remote: Receiver<Frame>,
    our_offer: String,
    remote_sdp: String,
    our_answer: Option<String>,
    local_texture: Option<TextureHandle>,
    remote_texture: Option<TextureHandle>,
}

impl RoomRTCApp {
    pub fn new() -> Self {
        let (tx_local, rx_local) = mpsc::channel();
        let (tx_remote, rx_remote) = mpsc::channel();

        Self {
            view: View::default(),
            controller: Controller::new(tx_local, tx_remote),
            rx_local,
            rx_remote,
            our_offer: String::new(),
            remote_sdp: String::new(),
            our_answer: None,
            local_texture: None,
            remote_texture: None,
        }
    }
}

impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);

            match self.view {
                View::Menu => self.show_menu(ui),
                View::Connection => self.show_connection(ui),
                View::Call => self.show_call(ctx, ui),
            }

            ctx.request_repaint();
        });
    }
}

impl RoomRTCApp {
    fn show_menu(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            let create_btn = egui::Button::new("Create Call (Offer)")
                .min_size(egui::vec2(200.0, 40.0));
            if ui.add_sized([200.0, 40.0], create_btn).clicked() {
                self.our_offer = self.controller.client.get_offer();
                self.remote_sdp = String::new();
                self.our_answer = None;
                self.view = View::Connection
            }

            ui.add_space(10.0);

            let join_btn = egui::Button::new("Join Call (Answer)")
                .min_size(egui::vec2(200.0, 40.0));
            if ui.add_sized([200.0, 40.0], join_btn).clicked() {
                self.our_offer = String::new();
                self.remote_sdp = String::new();
                self.our_answer = None;
                self.view = View::Connection;
            }
        });
    }

    fn show_connection(&mut self, ui: &mut Ui) {
        if !self.our_offer.is_empty() {
            self.offerer_flow(ui);
        } else {
            self.answerer_flow(ui);
        }

        if ui.button("Cancel").clicked() {
            self.view = View::Menu;
        }
    }

    fn show_call(&mut self, ctx: & Context, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Llamada");
            ui.add_space(20.0);

            self.update_video_textures(&ctx);

            ui.horizontal_centered(|ui| {
                ui.vertical(|ui| {
                    self.show_local_camera(ui);
                });
                ui.vertical(|ui| {
                    self.show_remote_camera(ui);
                });
            });

            let exit_btn = egui::Button::new("Finalizar llamada").min_size(egui::vec2(150.0, 40.0));
            if ui.add_sized([150.0, 40.0], exit_btn).clicked() {
                self.view = View::Menu;
            }
        });
    }

    fn offerer_flow(&mut self, ui: &mut Ui) {
        ui.heading("You are the Offerer");
        ui.separator();
        ui.label("1. Copy your offer and send it to the other user:");
        ui.add(egui::TextEdit::multiline(&mut self.our_offer.clone()));
        ui.separator();

        ui.label("2. Paste the remote user's answer below:");
        ui.add(
            egui::TextEdit::multiline(&mut self.remote_sdp).hint_text("Paste SDP Answer..."),
        );

        if !self.remote_sdp.is_empty() {
            if ui.button("Connect").clicked() {
                if let Err(e) = self.controller.client.process_answer(&self.remote_sdp) {
                    eprintln!("Failed to process answer: {}", e);
                    // TODO: Show this error in the GUI
                } else {
                    self.controller.start_call().unwrap();
                    self.view = View::Call;
                }
            }
        }
    }

    fn answerer_flow(&mut self, ui: &mut Ui) {
        ui.heading("You are the Answerer");
        ui.separator();
        ui.label("1. Paste the remote user's offer below:");
        ui.add(
            egui::TextEdit::multiline(&mut self.remote_sdp).hint_text("Paste SDP Offer..."),
        );

        if self.our_answer.is_none() {
            if !self.remote_sdp.is_empty() && ui.button("Generate Answer").clicked() {
                match self.controller.client.process_offer(&self.remote_sdp) {
                    Ok(answer_str) => self.our_answer = Some(answer_str),
                    Err(e) => eprintln!("Failed to process offer: {}", e),
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
                self.controller.start_call().unwrap();
                self.view = View::Call;
            }
        }
    }


    fn update_video_textures(&mut self, ctx: &Context) {
        update_camera_view(ctx, &self.rx_local, &mut self.local_texture, "local_camera");
        update_camera_view(ctx, &self.rx_remote, &mut self.remote_texture, "remote_camera");
    }

    fn show_local_camera(&mut self, ui: &mut Ui) {

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

    fn show_remote_camera(&mut self, ui: &mut Ui) {

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
