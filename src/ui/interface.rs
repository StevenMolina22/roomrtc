use crate::client::client::Client;

use super::views::View;
use eframe::egui;

pub struct RoomRTCApp {
    view: View,
    client: Option<Client>,
}

impl Default for RoomRTCApp {
    fn default() -> Self {
        Self {
            view: View::Menu,
            client: None,
        }
    }
}

impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);

            match &mut self.view {
                View::Menu => {
                    ui.vertical_centered(|ui| {
                        ui.heading("RoomRTC");
                        ui.add_space(20.0);

                        let create_btn = egui::Button::new("Create Call (Offer)")
                            .min_size(egui::vec2(200.0, 40.0));
                        if ui.add_sized([200.0, 40.0], create_btn).clicked() {
                            let client = Client::new();
                            let our_offer = client.get_offer();
                            self.client = Some(client);
                            self.view = View::Connecting {
                                our_offer,
                                remote_sdp: String::new(),
                                our_answer: None,
                            };
                        }

                        ui.add_space(10.0);

                        let join_btn = egui::Button::new("Join Call (Answer)")
                            .min_size(egui::vec2(200.0, 40.0));
                        if ui.add_sized([200.0, 40.0], join_btn).clicked() {
                            let client = Client::new();
                            self.client = Some(client);
                            self.view = View::Connecting {
                                our_offer: String::new(), // Empty indicates we are answerer
                                remote_sdp: String::new(),
                                our_answer: None,
                            };
                        }
                    });
                }

                View::Connecting {
                    our_offer,
                    remote_sdp,
                    our_answer,
                } => {
                    let client = self.client.as_mut().expect("Client should exist");

                    // --- Offerer Flow ---
                    if !our_offer.is_empty() {
                        ui.heading("You are the Offerer");
                        ui.separator();
                        ui.label("1. Copy your offer and send it to the other user:");
                        ui.add(egui::TextEdit::multiline(our_offer).interactive(false));
                        ui.separator();

                        ui.label("2. Paste the remote user's answer below:");
                        ui.add(
                            egui::TextEdit::multiline(remote_sdp).hint_text("Paste SDP Answer..."),
                        );

                        if !remote_sdp.is_empty() {
                            if ui.button("Connect").clicked() {
                                if let Err(e) = client.process_answer(remote_sdp) {
                                    eprintln!("Failed to process answer: {}", e);
                                    // TODO: Show this error in the GUI
                                } else {
                                    self.view = View::Call;
                                }
                            }
                        }
                    }
                    // --- Answerer Flow ---
                    else {
                        ui.heading("You are the Answerer");
                        ui.separator();
                        ui.label("1. Paste the remote user's offer below:");
                        ui.add(
                            egui::TextEdit::multiline(remote_sdp).hint_text("Paste SDP Offer..."),
                        );

                        if our_answer.is_none() {
                            if !remote_sdp.is_empty() && ui.button("Generate Answer").clicked() {
                                match client.process_offer(remote_sdp) {
                                    Ok(answer_str) => *our_answer = Some(answer_str),
                                    Err(e) => eprintln!("Failed to process offer: {}", e),
                                }
                            }
                        }

                        if let Some(answer_str) = our_answer {
                            ui.separator();
                            ui.label("2. Copy your answer and send it back:");
                            ui.add(egui::TextEdit::multiline(answer_str).interactive(false));
                            ui.label("Connection established. Waiting for remote...");
                            // We can go to 'Call' view immediately
                            // as our client logic already processed the offer.
                            self.view = View::Call;
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        self.client = None;
                        self.view = View::Menu;
                    }
                }

                View::Call => {
                    ui.vertical_centered(|ui| {
                        ui.heading("Call in Progress");
                        ui.add_space(20.0);
                        ui.label("TODO: Display Nokhwa camera feed and decoded video stream.");
                        ui.add_space(20.0);

                        let boton = egui::Button::new("Finalizar llamada")
                            .min_size(egui::vec2(150.0, 40.0));

                        if ui.add_sized([150.0, 40.0], boton).clicked() {
                            self.client = None;
                            self.view = View::Menu;
                        }
                    });
                }
            }
        });
    }
}
