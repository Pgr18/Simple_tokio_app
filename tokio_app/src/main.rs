use com_port_plotter::ui::MainWindow;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("COM Port Data Plotter (РКМ / РКМ-С)"),
        ..Default::default()
    };

    eframe::run_native(
        "COM Port Data Plotter",
        options,
        Box::new(|_cc| Box::new(MainWindow::new())),
    )
}
