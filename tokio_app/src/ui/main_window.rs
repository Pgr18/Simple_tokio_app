use eframe::egui;
use crate::com_port::{ComPortReader, SimpleProtocolParser};
use crate::data::processor::DataProcessor;
use super::plots::PlotManager;

pub struct MainWindow {
    com_reader: ComPortReader,
    data_processor: DataProcessor,
    plot_manager: PlotManager,
    selected_port: String,
    baud_rate: u32,
    is_connected: bool,
    available_ports: Vec<String>,
}

impl Default for MainWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl MainWindow {
    pub fn new() -> Self {
        let parser = Box::new(SimpleProtocolParser);
        let com_reader = ComPortReader::new(parser);
        let available_ports = ComPortReader::available_ports();
        
        Self {
            com_reader,
            data_processor: DataProcessor::new(1000),
            plot_manager: PlotManager::new(),
            selected_port: String::new(),
            baud_rate: 9600,
            is_connected: false,
            available_ports,
        }
    }

    fn update_ports_list(&mut self) {
        self.available_ports = ComPortReader::available_ports();
    }

    fn connect_disconnect(&mut self) {
        if self.is_connected {
            self.com_reader.disconnect();
            self.is_connected = false;
        } else if !self.selected_port.is_empty() {
            if let Err(e) = self.com_reader.connect(&self.selected_port, self.baud_rate) {
                eprintln!("Failed to connect: {}", e);
            } else {
                self.is_connected = true;
            }
        }
    }
}

impl eframe::App for MainWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Чтение данных из COM-порта
        if let Some(packet) = self.com_reader.read_data() {
            self.data_processor.add_packet(packet);
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("COM Port:");
                
                if ui.button("🔄").clicked() {
                    self.update_ports_list();
                }
                
                egui::ComboBox::from_id_source("port_selector")
                    .selected_text(if self.selected_port.is_empty() {
                        "Select port".to_string()
                    } else {
                        self.selected_port.clone()
                    })
                    .show_ui(ui, |ui| {
                        for port in &self.available_ports {
                            ui.selectable_value(&mut self.selected_port, port.clone(), port);
                        }
                    });

                ui.label("Baud rate:");
                ui.add(egui::DragValue::new(&mut self.baud_rate).speed(1).clamp_range(9600..=115200));

                let button_text = if self.is_connected { "Disconnect" } else { "Connect" };
                if ui.button(button_text).clicked() {
                    self.connect_disconnect();
                }

                if self.is_connected {
                    ui.colored_label(egui::Color32::GREEN, "● Connected");
                } else {
                    ui.colored_label(egui::Color32::RED, "● Disconnected");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.plot_manager.show_plots(ui, &self.data_processor);
        });

        // Запрос обновления для анимации
        ctx.request_repaint();
    }
}