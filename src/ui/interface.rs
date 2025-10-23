use crate::{
    client::client::{Client, VideoFrame},
    ui::views::View,
};
use eframe::egui;
use std::sync::mpsc::{self, Receiver};

pub struct RoomRTCApp {
    view: View,
    client: Option<Client>,
    local_video_receiver: Option<Receiver<VideoFrame>>,
    remote_video_receiver: Option<Receiver<VideoFrame>>,

    local_texture: Option<egui::TextureHandle>,
    remote_texture: Option<egui::TextureHandle>,
}

impl Default for RoomRTCApp {
    fn default() -> Self {
        Self {
            view: View::Menu,
            client: None,
            local_video_receiver: None,
            remote_video_receiver: None,
            local_texture: None,
            remote_texture: None,
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
                        // ...
                        let create_btn = egui::Button::new("Create Call (Offer)")
                            .min_size(egui::vec2(200.0, 40.0));
                        if ui.add_sized([200.0, 40.0], create_btn).clicked() {
                            // 1. Create channels
                            let (local_tx, local_rx) = mpsc::channel();
                            let (remote_tx, remote_rx) = mpsc::channel();

                            // 2. Pass Senders to Client
                            let client = Client::new(local_tx, remote_tx);
                            let our_offer = client.get_offer();

                            // 3. Store Client and Receivers in App
                            self.client = Some(client);
                            self.local_video_receiver = Some(local_rx);
                            self.remote_video_receiver = Some(remote_rx);

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
                            // 1. Create channels
                            let (local_tx, local_rx) = mpsc::channel();
                            let (remote_tx, remote_rx) = mpsc::channel();

                            // 2. Pass Senders to Client
                            let client = Client::new(local_tx, remote_tx);

                            // 3. Store Client and Receivers in App
                            self.client = Some(client);
                            self.local_video_receiver = Some(local_rx);
                            self.remote_video_receiver = Some(remote_rx);

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
                        ui.add(egui::TextEdit::multiline(&mut our_offer.clone()));
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
                            ui.add(egui::TextEdit::multiline(&mut answer_str.clone()));
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
                    // --- Receive Local Frame ---
                    if let Some(rx) = &self.local_video_receiver {
                        if let Ok(frame) = rx.try_recv() {
                            // This helper function is defined outside the update loop
                            update_texture_from_frame(ctx, frame, &mut self.local_texture);
                        }
                    }

                    // --- Receive Remote Frame ---
                    if let Some(rx) = &self.remote_video_receiver {
                        if let Ok(frame) = rx.try_recv() {
                            // This helper function is defined outside the update loop
                            update_texture_from_frame(ctx, frame, &mut self.remote_texture);
                        }
                    }

                    // --- Display Videos ---
                    ui.heading("Call in Progress");
                    ui.horizontal(|ui| {
                        // Display Remote Video
                        ui.vertical(|ui| {
                            ui.label("Remote Video");
                            if let Some(tex) = &self.remote_texture {
                                ui.add(egui::Image::new(tex).fit_to_exact_size(tex.size_vec2()));
                            } else {
                                ui.label("Waiting for remote video...");
                            }
                        });

                        // Display Local Video
                        ui.vertical(|ui| {
                            ui.label("Local Video (Self-View)");
                            if let Some(tex) = &self.local_texture {
                                ui.add(egui::Image::new(tex).fit_to_exact_size(tex.size_vec2()));
                            } else {
                                ui.label("Waiting for local camera...");
                            }
                        });
                    });

                    ui.add_space(20.0);

                    // --- Hang-up Button ---
                    let boton =
                        egui::Button::new("Finalizar llamada").min_size(egui::vec2(150.0, 40.0));

                    if ui.add_sized([150.0, 40.0], boton).clicked() {
                        self.client = None;
                        self.local_texture = None; // Clear textures
                        self.remote_texture = None; // Clear textures
                        self.local_video_receiver = None; // Drop receivers
                        self.remote_video_receiver = None;
                        self.view = View::Menu;
                    }

                    // Request repaint to keep the video feed "live"
                    ctx.request_repaint();
                }
            }
        });
    }
}

fn update_texture_from_frame(
    ctx: &egui::Context,
    frame: VideoFrame,
    texture: &mut Option<egui::TextureHandle>,
) {
    let image = egui::ColorImage::from_rgb(
        [frame.width as usize, frame.height as usize],
        &frame.rgb_data,
    );
    let tex = texture.get_or_insert_with(|| {
        ctx.load_texture(
            "video_frame",
            image.clone(),
            egui::TextureOptions::default(),
        )
    });
    tex.set(image, egui::TextureOptions::default());
}
