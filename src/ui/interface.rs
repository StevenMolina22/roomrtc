use super::views::View;
use eframe::egui;

/*
Little main to check how it works, it has to be removed from here

*/

#[derive(Default)]
pub struct RoomRTCApp {
    view: View,
}

impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);

            match &mut self.view {
                View::Menu => {
                    ui.vertical_centered(|ui| {
                        ui.heading("RoomRTC");
                        ui.add_space(20.0); // separación entre título y botón

                        // Botón de tamaño fijo (150x40 px)
                        let boton =
                            egui::Button::new("Comenzar llamada").min_size(egui::vec2(150.0, 40.0));

                        if ui.add_sized([150.0, 40.0], boton).clicked() {
                            self.view = View::Call;
                        }
                    });
                }
                View::Call => {
                    ui.vertical_centered(|ui| {
                        ui.heading("Llamada");
                        ui.add_space(20.0); // separación entre título y botón

                        // Botón de tamaño fijo (150x40 px)
                        let boton = egui::Button::new("Finalizar llamada")
                            .min_size(egui::vec2(150.0, 40.0));

                        if ui.add_sized([150.0, 40.0], boton).clicked() {
                            self.view = View::Menu;
                        }
                    });
                }
            }
        });
    }
}
