use eframe::egui;
use egui::Ui;
use super::views::View;
use crate::{controller::Controller};

pub struct RoomRTCApp {
    view: View,
    controller: Controller
}

impl RoomRTCApp {
    pub fn new() -> Self {
        Self { view: View::Menu, controller: Controller::new() }
    }
}

impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);

            match &mut self.view {
                View::Menu => self.show_menu(ui),
                View::Connection { our_offer, remote_sdp, our_answer } => {
                    self.show_connection(ui, our_offer, our_answer, remote_sdp);
                },
                View::Call => self.show_call(ui),
            }
        });
    }
}

impl RoomRTCApp {
    fn show_menu(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            // ...
            let create_btn = egui::Button::new("Create Call (Offer)")
                .min_size(egui::vec2(200.0, 40.0));
            if ui.add_sized([200.0, 40.0], create_btn).clicked() {
                self.view = View::Connection {
                    our_offer: self.controller.client.get_offer(),
                    remote_sdp: String::new(),
                    our_answer: None,
                };
            }

            ui.add_space(10.0);

            let join_btn = egui::Button::new("Join Call (Answer)")
                .min_size(egui::vec2(200.0, 40.0));
            if ui.add_sized([200.0, 40.0], join_btn).clicked() {
                self.view = View::Connection {
                    our_offer: String::new(),
                    remote_sdp: String::new(),
                    our_answer: None,
                };
            }
        });
    }

    fn show_connection(&mut self, ui: &mut Ui, our_offer: &mut String, our_answer: &mut Option<String>, remote_sdp: &mut String) {
        if !our_offer.is_empty() {
            self.offerer_flow(ui, our_offer, remote_sdp);
        } else {
            self.answerer_flow(ui, our_answer, remote_sdp);
        }

        if ui.button("Cancel").clicked() {
            self.view = View::Menu;
        }
    }

    fn show_call(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Llamada");
            ui.add_space(20.0);

            let boton = egui::Button::new("Finalizar llamada").min_size(egui::vec2(150.0, 40.0));

            if ui.add_sized([150.0, 40.0], boton).clicked() {
                self.view = View::Menu;
            }
        });
    }

    fn offerer_flow(&mut self, ui: &mut Ui, our_offer: &mut String, remote_sdp: &mut String) {
        ui.heading("You are the Offerer");
        ui.separator();
        ui.label("1. Copy your offer and send it to the other user:");
        ui.add(egui::TextEdit::multiline(&mut our_offer.clone()));
        ui.separator();

        ui.label("2. Paste the remote user's answer below:");
        ui.add(
            egui::TextEdit::multiline(remote_sdp).hint_text("Paste SDP Answer..."),
        );

        if !remote_sdp.is_empty() {
            if ui.button("Connect").clicked() {
                if let Err(e) = self.controller.client.process_answer(remote_sdp) {
                    eprintln!("Failed to process answer: {}", e);
                    // TODO: Show this error in the GUI
                } else {
                    self.view = View::Call;
                }
            }
        }
    }

    fn answerer_flow(&mut self, ui: &mut Ui, our_answer: &mut Option<String>, remote_sdp: &mut String) {
        ui.heading("You are the Answerer");
        ui.separator();
        ui.label("1. Paste the remote user's offer below:");
        ui.add(
            egui::TextEdit::multiline(remote_sdp).hint_text("Paste SDP Offer..."),
        );

        if our_answer.is_none() {
            if !remote_sdp.is_empty() && ui.button("Generate Answer").clicked() {
                match self.controller.client.process_offer(remote_sdp) {
                    Ok(answer_str) => *our_answer = Some(answer_str),
                    Err(e) => eprintln!("Failed to process offer: {}", e),
                }
            }
        }

        if let Some(answer_str) = our_answer {
            ui.separator();
            ui.label("2. Copy your answer and send it back:");
            ui.add(egui::TextEdit::multiline(&mut answer_str.clone()));
            ui.label("Connection established. Waiting for remote...");

            self.view = View::Call;
        }
    }
}
