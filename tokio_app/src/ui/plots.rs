use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use crate::data::processor::DataProcessor;

pub struct PlotManager;

impl PlotManager {
    pub fn new() -> Self {
        Self
    }

    pub fn show_plots(&mut self, ui: &mut egui::Ui, data_processor: &DataProcessor) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Канал A
            ui.heading("Channel A");
            Self::show_channel_plot(ui, data_processor.get_channel_a(), "Channel A");

            // Канал B
            ui.heading("Channel B");
            Self::show_channel_plot(ui, data_processor.get_channel_b(), "Channel B");

            // Канал C
            ui.heading("Channel C");
            Self::show_channel_plot(ui, data_processor.get_channel_c(), "Channel C");

            // Все каналы вместе
            ui.heading("All Channels");
            Self::show_combined_plot(ui, data_processor);
        });
    }

    fn show_channel_plot(ui: &mut egui::Ui, data: &[(u64, f64)], name: &str) {
        if data.is_empty() {
            ui.label("No data available");
            return;
        }

        let points: PlotPoints = data
            .iter()
            .map(|(timestamp, value)| [*timestamp as f64 * 0.001, *value])
            .collect();

        let line = Line::new(points).name(name);

        Plot::new(name)
            .height(300.0)
            .show_axes([true, true])
            .show(ui, |plot_ui| {
                plot_ui.line(line);
            });
    }

    fn show_combined_plot(ui: &mut egui::Ui, data_processor: &DataProcessor) {
        let plot = Plot::new("combined_plot")
            .height(400.0)
            .legend(egui_plot::Legend::default());

        plot.show(ui, |plot_ui| {
            // Канал A
            if !data_processor.get_channel_a().is_empty() {
                let points_a: PlotPoints = data_processor.get_channel_a()
                    .iter()
                    .map(|(timestamp, value)| [*timestamp as f64 * 0.001, *value])
                    .collect();
                let line_a = Line::new(points_a).name("Channel A");
                plot_ui.line(line_a);
            }

            // Канал B
            if !data_processor.get_channel_b().is_empty() {
                let points_b: PlotPoints = data_processor.get_channel_b()
                    .iter()
                    .map(|(timestamp, value)| [*timestamp as f64 * 0.001, *value])
                    .collect();
                let line_b = Line::new(points_b).name("Channel B");
                plot_ui.line(line_b);
            }

            // Канал C
            if !data_processor.get_channel_c().is_empty() {
                let points_c: PlotPoints = data_processor.get_channel_c()
                    .iter()
                    .map(|(timestamp, value)| [*timestamp as f64 * 0.001, *value])
                    .collect();
                let line_c = Line::new(points_c).name("Channel C");
                plot_ui.line(line_c);
            }
        });
    }
}