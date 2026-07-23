// В release не показывать чёрное окно консоли.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use com_port_plotter::ui::MainWindow;
use eframe::egui;
use std::sync::Arc;

fn load_app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/rheogram-icon.png"))
        .expect("app icon")
}

fn main() -> eframe::Result<()> {
    let icon = Arc::new(load_app_icon());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("COM Port Data Plotter (РКМ / РКМ-С)")
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "COM Port Data Plotter",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            // На случай, если система тянет тёмную тему.
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::light();
            cc.egui_ctx.set_style(style);
            Box::new(MainWindow::new())
        }),
    )
}
