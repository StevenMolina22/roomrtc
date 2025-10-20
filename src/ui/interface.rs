use eframe::egui;
use super::views::View;

/*
Little main to check how it works, it has to be removed from here
fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "RoomRTC App",
        options,
        Box::new(|_cc| Ok(Box::new(RoomRTCApp::default()))),
    )
}
*/

#[derive(Default)]
pub struct RoomRTCApp {
    vista_actual: View,
}


impl eframe::App for RoomRTCApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(40.0);

            match &mut self.vista_actual {
                View::Menu => {
                    ui.vertical_centered(|ui| {
                        ui.heading("RoomRTC");
                        ui.add_space(20.0); // separación entre título y botón

                        // Botón de tamaño fijo (150x40 px)
                        let boton = egui::Button::new("Comenzar llamada").min_size(egui::vec2(150.0, 40.0));

                        if ui.add_sized([150.0, 40.0], boton).clicked() {
                            self.vista_actual = View::Call;
                        }
                    });
                },
                View::Call => {
                    ui.vertical_centered(|ui| {
                        ui.heading("Llamada");
                        ui.add_space(20.0); // separación entre título y botón

                        // Botón de tamaño fijo (150x40 px)
                        let boton = egui::Button::new("Finalizar llamada").min_size(egui::vec2(150.0, 40.0));

                        if ui.add_sized([150.0, 40.0], boton).clicked() {
                            self.vista_actual = View::Menu;
                        }
                    });
                }
            }
        });
    }
}