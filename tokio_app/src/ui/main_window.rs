use eframe::egui;
use crate::com_port::{ComPortReader, SimpleProtocolParser};
use crate::data::processor::DataProcessor;
use super::plots::PlotManager;
use std::path::PathBuf;

pub struct MainWindow {
    com_reader: ComPortReader,
    data_processor: DataProcessor,
    plot_manager: PlotManager,
    selected_port: String,
    baud_rate: u32,
    is_connected: bool,
    available_ports: Vec<String>,
    save_path: PathBuf,
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
        
        // Устанавливаем путь для сохранения по умолчанию
        let mut save_path = std::env::current_dir().unwrap_or_default();
        save_path.push("recorded_data.csv");
        
        Self {
            com_reader,
            data_processor: DataProcessor::new(1000),
            plot_manager: PlotManager::new(),
            selected_port: String::new(),
            baud_rate: 9600,
            is_connected: false,
            available_ports,
            save_path,
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

    fn start_stop_recording(&mut self) {
        if self.data_processor.is_recording() {
            self.data_processor.stop_recording();
        } else {
            self.data_processor.start_recording();
        }
    }

    fn save_recorded_data(&mut self) {
        match self.data_processor.save_to_csv(&self.save_path) {
            Ok(()) => {
                println!("Data successfully saved to: {}", self.save_path.display());
                // Здесь можно добавить уведомление в UI
            }
            Err(e) => {
                eprintln!("Failed to save data: {}", e);
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
                // Секция COM-порта
                ui.vertical(|ui| {
                    ui.heading("COM Port");
                    ui.horizontal(|ui| {
                        ui.label("Port:");
                        
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

                        ui.label("Baud:");
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

                ui.separator();

                // Секция записи данных
                ui.vertical(|ui| {
                    ui.heading("Data Recording");
                    ui.horizontal(|ui| {
                        // Кнопка старт/стоп записи
                        let record_button_text = if self.data_processor.is_recording() { 
                            "⏹️ Stop Recording" 
                        } else { 
                            "⏺️ Start Recording" 
                        };
                        
                        // Создаем визуальное выделение для кнопки записи
                        let record_button = if self.data_processor.is_recording() {
                            ui.button(record_button_text)
                        } else {
                            ui.button(record_button_text)
                        };

                        if record_button.clicked() {
                            self.start_stop_recording();
                        }

                        // Статус записи
                        if self.data_processor.is_recording() {
                            ui.colored_label(egui::Color32::RED, "● Recording");
                            ui.label(format!("Points: {}", self.data_processor.get_recorded_count()));
                        } else {
                            ui.colored_label(egui::Color32::GRAY, "● Stopped");
                        }

                        // Кнопка сохранения (только когда не записываем)
                        let save_enabled = !self.data_processor.is_recording() && self.data_processor.get_recorded_count() > 0;
                        if ui.add_enabled(save_enabled, egui::Button::new("💾 Save to CSV")).clicked() {
                            self.save_recorded_data();
                        }

                        // Кнопка очистки (только когда не записываем)
                        let clear_enabled = !self.data_processor.is_recording() && self.data_processor.get_recorded_count() > 0;
                        if ui.add_enabled(clear_enabled, egui::Button::new("🗑️ Clear")).clicked() {
                            self.data_processor.clear_recorded_data();
                        }
                    });

                    // Поле пути для сохранения
                    ui.horizontal(|ui| {
                        ui.label("Save path:");
                        let mut path_str = self.save_path.to_string_lossy().to_string();
                        if ui.text_edit_singleline(&mut path_str).changed() {
                            self.save_path = PathBuf::from(path_str);
                        }
                        if ui.button("📁").clicked() {
                            // Здесь можно добавить диалог выбора файла
                        }
                    });
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.plot_manager.show_plots(ui, &self.data_processor);
        });

        // Запрос обновления для анимации
        ctx.request_repaint();
    }
}