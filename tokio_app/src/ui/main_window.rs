use eframe::egui;
use crate::com_port::{ComPortReader, SimpleProtocolParser};
use crate::data::processor::DataProcessor;
use super::plots::PlotManager;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use dirs::download_dir;

pub struct MainWindow {
    com_reader: ComPortReader,
    data_processor: DataProcessor,
    plot_manager: PlotManager,
    selected_port: String,
    baud_rate: u32,
    is_connected: bool,
    available_ports: Vec<String>,
    save_directory: PathBuf,
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
        
        // Устанавливаем директорию для сохранения по умолчанию: Downloads/gui-app
        let save_directory = Self::get_default_save_directory();
        
        Self {
            com_reader,
            data_processor: DataProcessor::new(1000),
            plot_manager: PlotManager::new(),
            selected_port: String::new(),
            baud_rate: 9600,
            is_connected: false,
            available_ports,
            save_directory,
        }
    }

    /// Получаем директорию для сохранения по умолчанию (Downloads/gui-app)
    fn get_default_save_directory() -> PathBuf {
        if let Some(mut downloads_dir) = download_dir() {
            downloads_dir.push("gui-app");
            // Создаем директорию, если её нет
            let _ = std::fs::create_dir_all(&downloads_dir);
            downloads_dir
        } else {
            // Fallback: текущая директория
            std::env::current_dir().unwrap_or_default()
        }
    }

    /// Генерирует полный путь к файлу для сохранения
    fn generate_save_path(&self) -> PathBuf {
        // Используем время начала записи, если оно есть
        if let Some(datetime_str) = self.data_processor.get_recording_start_datetime() {
            let filename = format!("{}.csv", datetime_str);
            self.save_directory.join(filename)
        } else {
            // Fallback: текущее время
            let now = chrono::Local::now();
            let filename = now.format("%Y-%m-%d %H-%M-%S.csv").to_string();
            self.save_directory.join(filename)
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

    fn save_recorded_data(&mut self) {
        if self.data_processor.get_recorded_count() == 0 {
            return;
        }

        let save_path = self.generate_save_path();
        
        match self.data_processor.save_to_csv(&save_path) {
            Ok(()) => {
                println!("Data successfully saved to: {}", save_path.display());
                self.data_processor.reset_recording();
            }
            Err(e) => {
                eprintln!("Failed to save data: {}", e);
            }
        }
        
    }

    fn select_save_directory(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select Save Directory")
            .set_directory(&self.save_directory)
            .pick_folder()
        {
            self.save_directory = path;
        }
    }

    /// Обработка горячих клавиш
    fn handle_hotkeys(&mut self, ctx: &egui::Context) {
        // В egui Key::S, Key::N, Key::R - это физические клавиши,
        // поэтому они работают независимо от раскладки
        
        // S - сохранение текущей записи
        if ctx.input(|i| i.key_pressed(egui::Key::S)) && 
           self.data_processor.get_recorded_count() > 0 {
            self.save_recorded_data();
        }
        
        // N - новая запись (сброс текущей)
        if ctx.input(|i| i.key_pressed(egui::Key::N)) {
            self.data_processor.reset_recording();
        }

        // R - переключение автозаписи
        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            if self.data_processor.is_auto_recording() {
                self.data_processor.stop_auto_recording();
            } else {
                self.data_processor.start_auto_recording();
            }
        }
    }
}

impl eframe::App for MainWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Обрабатываем горячие клавиши
        self.handle_hotkeys(ctx);

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
                    
                    // Статус и управление автозаписью
                    ui.horizontal(|ui| {
                        // Статус автозаписи
                        if self.data_processor.is_auto_recording() {
                            ui.colored_label(egui::Color32::GREEN, "● Auto Recording");
                        } else {
                            ui.colored_label(egui::Color32::YELLOW, "● Recording Paused");
                        }
                        
                        ui.label(format!("Points: {}", self.data_processor.get_recorded_count()));

                        // Кнопки управления автозаписью
                        if self.data_processor.is_auto_recording() {
                            if ui.button("⏸️ Pause (R)").clicked() {
                                self.data_processor.stop_auto_recording();
                            }
                        } else {
                            if ui.button("▶️ Resume (R)").clicked() {
                                self.data_processor.start_auto_recording();
                            }
                        }
                    });

                    // Дополнительная информация
                    ui.horizontal(|ui| {
                        // Время начала сессии записи
                        if let Some(datetime_str) = self.data_processor.get_recording_start_datetime() {
                            ui.label(format!("Session: {}", datetime_str));
                        } else if self.data_processor.is_auto_recording() {
                            ui.label("Session: Starting...");
                        } else {
                            ui.label("Session: Paused");
                        }

                        // Кнопка новой записи
                        if ui.button("🆕 New Session (N)").clicked() {
                            self.data_processor.reset_recording();
                        }

                        // Кнопка сохранения
                        let save_enabled = self.data_processor.get_recorded_count() > 0;
                        if ui.add_enabled(save_enabled, egui::Button::new("💾 Save (S)")).clicked() {
                            self.save_recorded_data();
                            
                        }
                    });

                    // Директория для сохранения
                    ui.horizontal(|ui| {
                        ui.label("Save directory:");
                        let mut dir_str = self.save_directory.to_string_lossy().to_string();
                        if ui.text_edit_singleline(&mut dir_str).changed() {
                            self.save_directory = PathBuf::from(dir_str);
                        }
                        if ui.button("📁").clicked() {
                            self.select_save_directory();
                        }
                    });

                    // Следующее имя файла
                    ui.horizontal(|ui| {
                        ui.label("Next filename:");
                        let next_filename = if let Some(datetime_str) = self.data_processor.get_recording_start_datetime() {
                            format!("{}.csv", datetime_str)
                        } else {
                            "No active session".to_string()
                        };
                        ui.label(next_filename);
                    });
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Подсказка по горячим клавишам
            ui.vertical_centered(|ui| {
                if self.data_processor.get_recorded_count() > 0 {
                    let recording_status = if self.data_processor.is_auto_recording() {
                        "● Auto Recording"
                    } else {
                        "● Recording Paused"
                    };
                    ui.colored_label(
                        if self.data_processor.is_auto_recording() { egui::Color32::GREEN } else { egui::Color32::YELLOW },
                        recording_status
                    );
                    ui.label("💡 Hotkeys: 'S' - Save, 'N' - New session, 'R' - Toggle recording");
                    ui.label("🎯 Hotkeys work in any keyboard layout!");
                }
            });
            
            self.plot_manager.show_plots(ui, &self.data_processor);
        });

        // Запрос обновления для анимации
        ctx.request_repaint();
    }
}