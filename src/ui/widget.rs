use crate::transport::rtcp::CallStats;
use crate::user::UserStatus;
use egui::{Button, Color32, Image, Response, RichText, Ui, include_image};

pub trait Widget {
    fn logo(&mut self, width: f32);
    fn title(&mut self, text: &str);
    fn text_input(&mut self, value: &mut String, hint: &str, password: bool) -> Response;
    fn primary_button(&mut self, text: &str, size: egui::Vec2) -> Response;
    fn neutral_button(&mut self, text: &str, size: egui::Vec2) -> Response;
    fn danger_button(&mut self, text: &str, size: egui::Vec2) -> Response;
    fn centered_and_sized_buttons<F>(
        &mut self,
        button_width: f32,
        n_buttons: usize,
        add_contents: F,
    ) where
        F: FnOnce(&mut Ui);
    fn user_profile_header(&mut self, username: &str, on_logout: impl FnOnce());
    fn contact_card(&mut self, username: &str, status: &UserStatus, on_call: impl FnOnce());
    fn contact_info(&mut self, username: &str, status: &UserStatus);
    fn contact_actions(&mut self, status: &UserStatus, on_call: impl FnOnce());
    fn call_action_button(&mut self, text: &str) -> Response;
    fn status_indicator(&mut self, status: &UserStatus);
    fn big_avatar(&mut self, icon: &str, size: f32);
    fn status_label(&mut self, text: &str, color: Color32);
    fn call_button(&mut self, text: &str, color: Color32) -> Response;
    fn call_action_btn(&mut self, text: &str, color: Color32, width: f32) -> Response;
    fn network_stats_panel(&mut self, stats: &CallStats);
    fn file_card<R>(&mut self, name: &str, size: u64, content: impl FnOnce(&mut Ui) -> R);
}

impl Widget for Ui {
    fn logo(&mut self, width: f32) {
        let logo_image = include_image!("assets/logo.png");
        self.add(
            Image::new(logo_image)
                .max_width(width)
                .maintain_aspect_ratio(true),
        );
    }

    fn title(&mut self, text: &str) {
        self.label(
            RichText::new(text)
                .size(32.0)
                .strong()
                .color(Color32::WHITE),
        );
    }

    fn text_input(&mut self, value: &mut String, hint: &str, password: bool) -> Response {
        let edit = egui::TextEdit::singleline(value)
            .password(password)
            .hint_text(RichText::new(hint).size(16.0))
            .margin(egui::vec2(10.0, 10.0));
        self.add_sized([280.0, 35.0], edit)
    }

    fn primary_button(&mut self, text: &str, size: egui::Vec2) -> Response {
        let rich_text = RichText::new(text)
            .size(18.0)
            .strong()
            .color(Color32::WHITE);
        self.add_sized(
            size,
            Button::new(rich_text).fill(Color32::from_rgb(45, 120, 255)),
        )
    }

    fn neutral_button(&mut self, text: &str, size: egui::Vec2) -> Response {
        let rich_text = RichText::new(text).size(16.0).color(Color32::LIGHT_GRAY);
        self.add_sized(
            size,
            Button::new(rich_text).fill(Color32::from_rgb(60, 60, 60)),
        )
    }

    fn danger_button(&mut self, text: &str, size: egui::Vec2) -> Response {
        let rich_text = RichText::new(text)
            .size(18.0)
            .strong()
            .color(Color32::WHITE);
        self.add_sized(
            size,
            Button::new(rich_text).fill(Color32::from_rgb(180, 50, 50)),
        )
    }

    fn centered_and_sized_buttons<F>(
        &mut self,
        button_width: f32,
        n_buttons: usize,
        add_contents: F,
    ) where
        F: FnOnce(&mut Ui),
    {
        let spacing = self.spacing().item_spacing.x;
        let total_width = (button_width * n_buttons as f32) + (spacing * (n_buttons - 1) as f32);
        let x_offset = (self.available_width() - total_width) / 2.0;
        self.horizontal(|ui| {
            ui.add_space(x_offset);
            add_contents(ui);
        });
    }

    fn user_profile_header(&mut self, username: &str, on_logout: impl FnOnce()) {
        self.horizontal(|ui| {
            ui.label(RichText::new("👤").size(22.0));
            ui.label(
                RichText::new(username)
                    .strong()
                    .size(18.0)
                    .color(Color32::WHITE),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let logout_img = include_image!("assets/exit_logo.png");
                if ui
                    .add(
                        Button::image(
                            Image::new(logout_img).fit_to_exact_size(egui::vec2(40.0, 40.0)),
                        )
                        .fill(Color32::TRANSPARENT)
                        .frame(false),
                    )
                    .on_hover_text("Log Out")
                    .clicked()
                {
                    on_logout();
                }
            });
        });
    }

    fn contact_card(&mut self, username: &str, status: &UserStatus, on_call: impl FnOnce()) {
        egui::Frame::new()
            .fill(Color32::from_rgb(20, 20, 20))
            .inner_margin(8.0)
            .show(self, |ui| {
                ui.horizontal(|ui| {
                    ui.contact_info(username, status);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.contact_actions(status, on_call);
                    });
                });
            });
    }

    fn contact_info(&mut self, username: &str, status: &UserStatus) {
        self.status_indicator(status);
        self.vertical(|ui| {
            ui.label(
                RichText::new(username)
                    .strong()
                    .color(Color32::WHITE)
                    .size(16.0),
            );
            let status_string = status.to_string();
            let status_label_text = match status_string.split(':').next() {
                Some(s) => s.to_owned(),
                None => String::new(),
            };
            ui.label(
                RichText::new(status_label_text).size(11.0).color(Color32::GRAY),
            );
        });
    }

    fn contact_actions(&mut self, status: &UserStatus, on_call: impl FnOnce()) {
        match status {
            UserStatus::Available => {
                if self.call_action_button("Call").clicked() {
                    on_call();
                }
            }
            UserStatus::Occupied(peer) => {
                self.label(
                    RichText::new(format!("In call with {peer}"))
                        .size(12.0)
                        .italics()
                        .color(Color32::GRAY),
                );
            }
            UserStatus::Offline => {}
        }
    }

    fn call_action_button(&mut self, text: &str) -> Response {
        let text = RichText::new(text).color(Color32::WHITE).strong();
        self.add_sized(
            [70.0, 28.0],
            Button::new(text).fill(Color32::from_rgb(0, 122, 255)),
        )
    }

    fn status_indicator(&mut self, status: &UserStatus) {
        let color = match status {
            UserStatus::Available => Color32::from_rgb(50, 200, 50),
            UserStatus::Offline => Color32::from_rgb(60, 60, 60),
            UserStatus::Occupied(_) => Color32::from_rgb(200, 50, 50),
        };
        let (rect, _) = self.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        self.painter().circle_filled(rect.center(), 6.0, color);
        self.add_space(5.0);
    }

    fn big_avatar(&mut self, icon: &str, size: f32) {
        let radius = size / 2.0;
        let (rect, _) = self.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());

        self.painter()
            .circle_filled(rect.center(), radius, Color32::from_rgb(30, 30, 30));

        self.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(size * 0.5),
            Color32::LIGHT_GRAY,
        );
    }

    fn status_label(&mut self, text: &str, color: Color32) {
        self.label(RichText::new(text).size(20.0).color(color));
    }

    fn call_button(&mut self, text: &str, color: Color32) -> Response {
        let rich_text = RichText::new(text)
            .size(18.0)
            .strong()
            .color(Color32::WHITE);
        self.add_sized([130.0, 60.0], Button::new(rich_text).fill(color))
    }
    fn call_action_btn(&mut self, text: &str, color: Color32, width: f32) -> Response {
        let rich_text = RichText::new(text).strong().color(Color32::WHITE);
        self.add_sized([width, 45.0], Button::new(rich_text).fill(color))
    }

    fn network_stats_panel(&mut self, stats: &CallStats) {
        egui::Frame::new()
            .fill(Color32::from_rgb(25, 25, 25))
            .inner_margin(12.0)
            .show(self, |ui| {
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
                        let color = if stats.remote_receiver.packets_lost > 0 {
                            Color32::KHAKI
                        } else {
                            Color32::GREEN
                        };
                        ui.label(
                            RichText::new(format!("{}", stats.remote_receiver.packets_lost))
                                .color(color),
                        );
                        ui.end_row();
                        ui.label(RichText::new("Sent / Received:").color(Color32::GRAY));
                        ui.label(format!(
                            "{} / {}",
                            stats.local_sender.packets_sent, stats.local_receiver.packets_received
                        ));
                        ui.end_row();
                    });
            });
    }

    fn file_card<R>(&mut self, name: &str, size: u64, content: impl FnOnce(&mut Ui) -> R) {
        egui::Frame::group(self.style())
            .fill(Color32::from_rgb(35, 35, 35))
            .inner_margin(8.0)
            .show(self, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new(name).strong().color(Color32::WHITE));
                ui.label(
                    RichText::new(format!("{} B", size))
                        .size(11.0)
                        .color(Color32::GRAY),
                );
                ui.add_space(4.0);
                content(ui);
            });
        self.add_space(8.0);
    }
}
