mod com_port;
mod data;
mod ui;

use eframe::egui;
use ui::MainWindow;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("COM Port Data Plotter"),
        ..Default::default()
    };

    eframe::run_native(
        "COM Port Data Plotter",
        options,
        Box::new(|cc| {
            // Убираем строку с egui_extras, если она не нужна
            // или используем альтернативный подход
            Box::new(MainWindow::new())
        }),
    )
}