use std::net::UdpSocket;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use eframe::egui;
use egui::Ui;
use super::views::View;
use crate::{controller::Controller};

pub struct RoomRTCApp {
    view: View,
    controller: Controller,
    rx_local: Receiver<Vec<u8>>,
    rx_remote: Receiver<Vec<u8>>,
    socket: Option<UdpSocket>,
    addr: String,
    our_offer: String,
    remote_sdp: String,
    our_answer: Option<String>,
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
            socket: None,
            addr: String::new(),
            our_offer: String::new(),
            remote_sdp: String::new(),
            our_answer: None,
        }
    }
}

impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);

            match self.view {
                View::Menu => self.show_menu(ui),
                View::Connection => self.show_connection(ui),
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

    fn show_call(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Llamada");
            ui.add_space(20.0);

            let boton = egui::Button::new("Finalizar llamada").min_size(egui::vec2(150.0, 40.0));
            let msg_btn = egui::Button::new("send").min_size(egui::vec2(150.0, 40.0));

            if self.socket.is_none() && let Some(pair) = self.controller.client.ice_agent.get_selected_pair() {
                let local_addr = format!("{}:{}", pair.local.address, pair.local.port);
                let remote_addr = format!("{}:{}", pair.remote.address, pair.remote.port);
                self.addr = local_addr.clone();

                let socket = UdpSocket::bind(local_addr).unwrap();
                socket.connect(remote_addr).unwrap();
                socket.set_nonblocking(true).unwrap();
                self.socket = Some(socket);


            }
            if let Some(socket) = &self.socket {
                let mut buf = [0u8; 1024];
                if !socket.recv(&mut buf).is_err() {
                    println!("{}", String::from_utf8(buf.to_vec()).unwrap());
                }

                if ui.add(msg_btn).clicked() {
                    socket.send(format!("hola soy {}", self.addr).as_bytes()).unwrap();
                }

                if ui.add_sized([150.0, 40.0], boton).clicked() {
                    self.view = View::Menu;
                }
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
                self.view = View::Call;
            }
        }
    }
}
