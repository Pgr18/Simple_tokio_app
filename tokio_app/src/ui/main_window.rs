use eframe::egui;
use crate::com_port::{ComPortReader, SimpleProtocolParser};
use crate::data::processor::DataProcessor;
use super::plots::{PlotManager, TimeScale};
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
        
        // Устанавливаем директорию для сохранения по умолчанию: Downloads/gui-app/ГГГГ-ММ-ДД
        let save_directory = Self::get_default_save_directory();
        
        // Увеличиваем размер буфера для отображения до 6000 точек (100 точек/секунду × 60 секунд)
        let display_buffer_size = 6000;
        
        Self {
            com_reader,
            data_processor: DataProcessor::new(display_buffer_size),
            plot_manager: PlotManager::new(),
            selected_port: String::new(),
            baud_rate: 9600,
            is_connected: false,
            available_ports,
            save_directory,
        }
    }

    /// Получаем директорию для сохранения по умолчанию (Downloads/gui-app/ГГГГ-ММ-ДД)
    fn get_default_save_directory() -> PathBuf {
        if let Some(mut downloads_dir) = download_dir() {
            downloads_dir.push("gui-app");
            
            // Добавляем текущую дату в формате ГГГГ-ММ-ДД
            let today = chrono::Local::now();
            let date_folder = today.format("%Y-%m-%d").to_string();
            downloads_dir.push(date_folder);
            
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
        // Используем время начала записи и номер сессии
        if let Some(datetime_str) = self.data_processor.get_recording_start_datetime() {
            let session_number = self.data_processor.get_session_counter();
            let filename = format!("{} session_{}.csv", datetime_str, session_number);
            self.save_directory.join(filename)
        } else {
            // Fallback: текущее время и номер сессии
            let now = chrono::Local::now();
            let session_number = self.data_processor.get_session_counter();
            let filename = format!("{} session_{}.csv", now.format("%Y-%m-%d %H-%M-%S"), session_number);
            self.save_directory.join(filename)
        }
    }

    /// Обновляет директорию сохранения на текущую дату
    fn update_save_directory_to_today(&mut self) {
        if let Some(mut downloads_dir) = download_dir() {
            downloads_dir.push("gui-app");
            
            // Добавляем текущую дату в формате ГГГГ-ММ-ДД
            let today = chrono::Local::now();
            let date_folder = today.format("%Y-%m-%d").to_string();
            downloads_dir.push(date_folder);
            
            // Создаем директорию, если её нет
            let _ = std::fs::create_dir_all(&downloads_dir);
            self.save_directory = downloads_dir;
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

        // Перед сохранением обновляем директорию на текущую дату
        self.update_save_directory_to_today();

        let save_path = self.generate_save_path();
        
        match self.data_processor.save_to_csv(&save_path) {
            Ok(()) => {
                println!("Data successfully saved to: {}", save_path.display());
                // После успешного сохранения сбрасываем запись для начала новой сессии
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

    /// Проверяет нажатие клавиши по scan code (работает в любой раскладке)
    fn is_key_pressed_by_scan_code(ctx: &egui::Context, target_scan_code: u32) -> bool {
        ctx.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::Key { key: _, physical_key: Some(physical_key), pressed, .. } = event {
                    if *pressed {
                        // Сравниваем физические коды клавиш
                        match physical_key {
                            egui::Key::S if target_scan_code == 31 => return true,  // S
                            egui::Key::N if target_scan_code == 49 => return true,  // N
                            egui::Key::R if target_scan_code == 19 => return true,  // R
                            egui::Key::Plus if target_scan_code == 46 => return true, // +
                            egui::Key::Minus if target_scan_code == 45 => return true, // -
                            _ => {}
                        }
                    }
                }
            }
            false
        })
    }

    /// Альтернативный способ: обработка через текст (для символов)
    fn is_key_pressed_by_char(ctx: &egui::Context, target_chars: &[char]) -> bool {
        ctx.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::Text(text) = event {
                    for c in text.chars() {
                        if target_chars.contains(&c) {
                            return true;
                        }
                    }
                }
            }
            false
        })
    }

    /// Обработка горячих клавиш (работает в любой раскладке)
    fn handle_hotkeys(&mut self, ctx: &egui::Context) {
        // Способ 2: Альтернативный - по символам (для разных раскладок)
        // S/Ы - сохранение
        if Self::is_key_pressed_by_char(ctx, &['s', 'S', 'ы', 'Ы']) && 
           self.data_processor.get_recorded_count() > 0 {
            self.save_recorded_data();
        }
        
        // N/Т - новая запись
        if Self::is_key_pressed_by_char(ctx, &['n', 'N', 'т', 'Т']) {
            self.data_processor.reset_recording();
        }

        // R/К - переключение автозаписи
        if Self::is_key_pressed_by_char(ctx, &['r', 'R', 'к', 'К']) {
            if self.data_processor.is_auto_recording() {
                self.data_processor.stop_auto_recording();
            } else {
                self.data_processor.start_auto_recording();
            }
        }

        // +/- - масштабирование
        if Self::is_key_pressed_by_char(ctx, &['+', '=', '±', '₊']) {
            self.plot_manager.zoom_in();
        }

        if Self::is_key_pressed_by_char(ctx, &['-', '_', '–', '—']) {
            self.plot_manager.zoom_out();
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
                        ui.label(format!("Session: #{}", self.data_processor.get_session_counter()));

                        // Кнопки управления автозаписью
                        if self.data_processor.is_auto_recording() {
                            if ui.button("⏸️ Pause (R/К)").clicked() {
                                self.data_processor.stop_auto_recording();
                            }
                        } else {
                            if ui.button("▶️ Resume (R/К)").clicked() {
                                self.data_processor.start_auto_recording();
                            }
                        }
                    });

                    // Дополнительная информация
                    ui.horizontal(|ui| {
                        // Время начала сессии записи
                        if let Some(datetime_str) = self.data_processor.get_recording_start_datetime() {
                            ui.label(format!("Started: {}", datetime_str));
                        } else if self.data_processor.is_auto_recording() {
                            ui.label("Started: Starting...");
                        } else {
                            ui.label("Started: Paused");
                        }

                        // Кнопка новой записи
                        if ui.button("🆕 New Session (N/Т)").clicked() {
                            self.data_processor.reset_recording();
                        }

                        // Кнопка сохранения (со сбросом после сохранения)
                        let save_enabled = self.data_processor.get_recorded_count() > 0;
                        if ui.add_enabled(save_enabled, egui::Button::new("💾 Save & Reset (S/Ы)")).clicked() {
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
                        // Кнопка для возврата к папке с текущей датой
                        if ui.button("📅 Today").clicked() {
                            self.update_save_directory_to_today();
                        }
                    });

                    // Следующее имя файла (всегда актуальное)
                    ui.horizontal(|ui| {
                        ui.label("Next filename:");
                        let next_filename = if let Some(datetime_str) = self.data_processor.get_recording_start_datetime() {
                            let session_number = self.data_processor.get_session_counter();
                            format!("{} session_{}.csv", datetime_str, session_number)
                        } else {
                            "No active session".to_string()
                        };
                        ui.label(next_filename);
                        
                        // Показываем полный путь для информации
                        ui.label(format!("→ {}", self.generate_save_path().display()));
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
                    let time_scale = self.plot_manager.get_time_scale();
                    ui.colored_label(
                        if self.data_processor.is_auto_recording() { egui::Color32::GREEN } else { egui::Color32::YELLOW },
                        recording_status
                    );
                    ui.label(format!("📊 Time scale: {} seconds", time_scale.get_seconds()));
                    ui.label("💡 Hotkeys: 'S/Ы' - Save, 'N/Т' - New session, 'R/К' - Toggle recording");
                    ui.label("🔍 Zoom: '+' - Zoom in (less time), '-' - Zoom out (more time)");
                    ui.label("🎯 Hotkeys work in any keyboard layout!");
                } else {
                    ui.label("📊 Waiting for data... Start recording to begin session");
                }
            });
            
            self.plot_manager.show_plots(ui, &self.data_processor);
        });

        // Запрос обновления для анимации
        ctx.request_repaint();
    }
}