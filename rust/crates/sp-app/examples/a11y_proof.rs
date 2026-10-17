//! The toolkit proof (docs/superpowers/specs/2026-10-11-sp-app-design.md):
//! one window, one styled button, one text field, one custom-painted panel.
//! Run it, then read the window's accessibility tree from outside the
//! process with `rust/crates/sp-app/tools/ax-tree.applescript`: the button's
//! name must be there, as the screen reader would hear it.

use eframe::egui;

const BRASS: egui::Color32 = egui::Color32::from_rgb(0xe8, 0xa3, 0x3d);
const INK: egui::Color32 = egui::Color32::from_rgb(0x1d, 0x1b, 0x16);
const PAPER: egui::Color32 = egui::Color32::from_rgb(0xf7, 0xf2, 0xe6);

struct Proof {
    field: String,
    presses: u32,
}

impl eframe::App for Proof {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(PAPER).inner_margin(24.0))
            .show(ui, |ui| {
                let button =
                    egui::Button::new(egui::RichText::new("CHOOSE PDF…").color(INK).strong())
                        .fill(BRASS)
                        .corner_radius(3.0);
                if ui.add(button).clicked() {
                    self.presses += 1;
                }
                ui.label(format!("Pressed {} times", self.presses));
                ui.add_space(12.0);
                ui.label("Which device?");
                ui.add(egui::TextEdit::singleline(&mut self.field).hint_text("Device name"));
                ui.add_space(12.0);
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(120.0, 120.0), egui::Sense::hover());
                let painter = ui.painter_at(rect);
                painter.circle_filled(
                    rect.center() + egui::vec2(3.0, 4.0),
                    47.0,
                    egui::Color32::from_black_alpha(80),
                );
                painter.circle_filled(
                    rect.center(),
                    41.0,
                    egui::Color32::from_rgb(0x7a, 0x51, 0x16),
                );
                painter.circle_filled(rect.center(), 39.5, BRASS);
                painter.circle_filled(
                    rect.center() - egui::vec2(12.0, 17.0),
                    12.0,
                    egui::Color32::from_white_alpha(150),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Other,
                        true,
                        "A brass brad, painted",
                    )
                });
            });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Method Reader proof")
            .with_inner_size([480.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Method Reader proof",
        options,
        Box::new(|_| {
            Ok(Box::new(Proof {
                field: String::new(),
                presses: 0,
            }))
        }),
    )
}
