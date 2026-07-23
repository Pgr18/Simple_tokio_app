use eframe::egui;
use crate::com_port::{find_rcm_port_among, list_port_names, ProbeResult};
use crate::data::bin_playback::{guess_profile_from_path, load_bin_file, BinLoadResult};
use crate::data::live_worker::{LiveEvent, LiveWorker};
use crate::data::processor::DataProcessor;
use crate::data::RcmProfile;
use super::plots::PlotManager;
use dirs::download_dir;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

enum DiscoverMsg {
    Done(Result<ProbeResult, String>),
}

enum BinMsg {
    Done(Result<BinLoadResult, String>),
}

pub struct MainWindow {
    data_processor: DataProcessor,
    plot_manager: PlotManager,
    live: Option<LiveWorker>,
    selected_port: String,
    baud_rate: u32,
    is_connected: bool,
    available_ports: Vec<String>,
    save_directory: PathBuf,
    port_status: String,
    discovering: bool,
    discover_rx: Option<Receiver<DiscoverMsg>>,
    auto_find_on_start: bool,
    loading_bin: bool,
    bin_rx: Option<Receiver<BinMsg>>,
    bin_status: String,
}

impl Default for MainWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl MainWindow {
    pub fn new() -> Self {
        let available_ports = list_port_names();
        let save_directory = Self::get_default_save_directory();
        let display_buffer_size = 14_000;
        let selected_port = available_ports
            .iter()
            .find(|p| p.eq_ignore_ascii_case("COM3"))
            .cloned()
            .or_else(|| {
                if available_ports.len() == 1 {
                    available_ports.first().cloned()
                } else {
                    None
                }
            })
            .unwrap_or_default();

        Self {
            data_processor: DataProcessor::new(display_buffer_size),
            plot_manager: PlotManager::new(),
            live: None,
            selected_port,
            baud_rate: 38400,
            is_connected: false,
            available_ports,
            save_directory,
            port_status: "Готов к автопоиску прибора".into(),
            discovering: false,
            discover_rx: None,
            auto_find_on_start: true,
            loading_bin: false,
            bin_rx: None,
            bin_status: String::new(),
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
        self.available_ports = list_port_names();
    }

    fn stop_live(&mut self) {
        if let Some(live) = self.live.take() {
            live.stop();
        }
        self.is_connected = false;
    }

    fn start_live(&mut self, port: &str) -> Result<(), String> {
        self.stop_live();
        self.data_processor.clear();
        self.plot_manager.enter_live_view();
        self.plot_manager.reset_y_bounds();
        let worker = LiveWorker::start(
            port.to_string(),
            self.baud_rate,
            self.data_processor.profile(),
            self.data_processor.filters_enabled(),
        )?;
        self.live = Some(worker);
        self.is_connected = true;
        self.selected_port = port.to_string();
        Ok(())
    }

    fn connect_disconnect(&mut self) {
        if self.is_connected {
            self.stop_live();
            self.port_status = "Отключено".into();
            return;
        }

        self.update_ports_list();

        if !self.selected_port.is_empty() {
            let port = self.selected_port.clone();
            match self.start_live(&port) {
                Ok(()) => self.port_status = format!("Подключено: {port} [7N1]"),
                Err(e) => {
                    self.port_status = format!("Ошибка подключения {port}: {e}");
                    eprintln!("Failed to connect: {e}");
                }
            }
            return;
        }

        self.start_auto_discover();
    }

    /// Запускает поиск прибора в фоне (только 7N1).
    fn start_auto_discover(&mut self) {
        if self.discovering {
            return;
        }
        if self.is_connected {
            self.stop_live();
        }

        self.update_ports_list();
        let ports = self.available_ports.clone();
        if ports.is_empty() {
            self.port_status = "COM-порты не найдены".into();
            return;
        }

        if ports.len() == 1 || !self.selected_port.is_empty() {
            let port = if !self.selected_port.is_empty() {
                self.selected_port.clone()
            } else {
                ports[0].clone()
            };
            match self.start_live(&port) {
                Ok(()) => self.port_status = format!("Подключено: {port} [7N1]"),
                Err(e) => self.port_status = format!("Не открыть {port}: {e}"),
            }
            return;
        }

        let baud = self.baud_rate;
        let (tx, rx) = mpsc::channel();
        self.discover_rx = Some(rx);
        self.discovering = true;
        self.port_status = format!("Поиск прибора на {} портах [7N1]…", ports.len());

        std::thread::spawn(move || {
            let result = match find_rcm_port_among(ports, baud) {
                Some(probe) => Ok(probe),
                None => Err("Прибор РКМ/РКМ-С не найден".into()),
            };
            let _ = tx.send(DiscoverMsg::Done(result));
        });
    }

    fn poll_discover(&mut self) {
        let Some(rx) = self.discover_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(DiscoverMsg::Done(Ok(probe))) => {
                self.discovering = false;
                self.discover_rx = None;
                let port = probe.port_name.clone();
                let label = probe.config.label();
                let frames = probe.frames;
                match self.start_live(&port) {
                    Ok(()) => {
                        self.port_status = format!(
                            "Найден и подключен: {port} [{label}] ({frames} кадров при пробе)"
                        );
                    }
                    Err(e) => {
                        self.port_status = format!("Найден {port}, но не удалось открыть: {e}");
                    }
                }
            }
            Ok(DiscoverMsg::Done(Err(msg))) => {
                self.discovering = false;
                self.discover_rx = None;
                if self.available_ports.len() == 1 {
                    let port = self.available_ports[0].clone();
                    match self.start_live(&port) {
                        Ok(()) => {
                            self.port_status = format!(
                                "Автопоиск не уверен, открыт единственный порт: {port} [7N1]"
                            );
                        }
                        Err(e) => {
                            self.port_status = format!("{msg}; также не открыть {port}: {e}");
                        }
                    }
                } else {
                    self.port_status = msg;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.discovering = false;
                self.discover_rx = None;
                self.port_status = "Поиск прерван".into();
            }
        }
    }

    fn poll_live(&mut self) {
        let Some(live) = self.live.as_ref() else {
            return;
        };
        let events = live.poll();
        let mut fatal = None;
        for ev in events {
            match ev {
                LiveEvent::Batch(points) => {
                    for p in &points {
                        self.data_processor.push_live_point(p);
                    }
                }
                LiveEvent::Error(msg) => {
                    fatal = Some(msg);
                }
            }
        }
        if let Some(msg) = fatal {
            self.stop_live();
            self.port_status = msg;
        }
    }

    fn open_bin_dialog(&mut self) {
        if self.loading_bin {
            return;
        }
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
        let dialog = rfd::FileDialog::new()
            .set_title("Открыть сырой дамп РКМ/РКМ-С (.bin)")
            .add_filter("RCM binary", &["bin"])
            .add_filter("All", &["*"]);
        let dialog = if fixtures.is_dir() {
            dialog.set_directory(&fixtures)
        } else {
            dialog
        };
        let Some(path) = dialog.pick_file() else {
            return;
        };

        // Остановить live COM — смотрим файл.
        self.stop_live();

        let guessed = guess_profile_from_path(&path);
        if guessed != self.data_processor.profile() {
            self.data_processor.set_profile(guessed);
        }
        let profile = self.data_processor.profile();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let (tx, rx) = mpsc::channel();
        self.bin_rx = Some(rx);
        self.loading_bin = true;
        self.bin_status = format!("Загрузка {name}…");
        self.port_status = self.bin_status.clone();

        std::thread::spawn(move || {
            let result = load_bin_file(&path, profile);
            let _ = tx.send(BinMsg::Done(result));
        });
    }

    fn poll_bin_load(&mut self) {
        let Some(rx) = self.bin_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(BinMsg::Done(Ok(result))) => {
                self.loading_bin = false;
                self.bin_rx = None;
                self.apply_bin_result(result);
            }
            Ok(BinMsg::Done(Err(e))) => {
                self.loading_bin = false;
                self.bin_rx = None;
                self.bin_status = format!("Ошибка .bin: {e}");
                self.port_status = self.bin_status.clone();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.loading_bin = false;
                self.bin_rx = None;
                self.bin_status = "Загрузка .bin прервана".into();
                self.port_status = self.bin_status.clone();
            }
        }
    }

    fn apply_bin_result(&mut self, result: BinLoadResult) {
        let stats = &result.stats;
        // Храним весь дамп на графике (с потолком по памяти).
        let need = result.points.len().saturating_add(1000).min(500_000).max(14_000);
        self.data_processor.set_max_points(need);
        self.data_processor.clear();

        for p in &result.points {
            self.data_processor.push_live_point(p);
        }

        self.plot_manager
            .enter_offline_view(self.data_processor.get_max_time());

        let name = stats
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| stats.path.display().to_string());
        self.bin_status = format!(
            "{name}: {} байт → {} кадров, resync={}, {:.1} с (сырой decode, без фильтров)",
            stats.bytes, stats.frames, stats.resyncs, stats.duration_s
        );
        self.port_status = self.bin_status.clone();
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
        if self.auto_find_on_start {
            self.auto_find_on_start = false;
            self.start_auto_discover();
        }

        self.poll_discover();
        self.poll_live();
        self.poll_bin_load();
        if self.discovering || self.is_connected || self.loading_bin {
            // Java Timeline ~50 ms (~20 FPS); при загрузке bin тоже крутим UI.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        self.handle_hotkeys(ctx);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
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
                        ui.add(
                            egui::DragValue::new(&mut self.baud_rate)
                                .speed(100)
                                .clamp_range(1200..=115200),
                        );

                        let find_enabled = !self.discovering && !self.is_connected && !self.loading_bin;
                        if ui
                            .add_enabled(find_enabled, egui::Button::new("🔍 Автопоиск"))
                            .on_hover_text("Сканирует COM-порты и ищет поток кадров РКМ/РКМ-С")
                            .clicked()
                        {
                            self.start_auto_discover();
                        }

                        if ui
                            .add_enabled(!self.loading_bin, egui::Button::new("📂 Open .bin"))
                            .on_hover_text("Открыть сырой дамп (как tests/fixtures/*.bin) и показать на графиках")
                            .clicked()
                        {
                            self.open_bin_dialog();
                        }

                        let button_text = if self.is_connected {
                            "Disconnect"
                        } else {
                            "Connect"
                        };
                        if ui
                            .add_enabled(!self.discovering && !self.loading_bin, egui::Button::new(button_text))
                            .clicked()
                        {
                            self.connect_disconnect();
                        }

                        if self.loading_bin {
                            ui.spinner();
                            ui.colored_label(egui::Color32::LIGHT_BLUE, "● Loading .bin");
                        } else if self.discovering {
                            ui.spinner();
                            ui.colored_label(egui::Color32::YELLOW, "● Searching");
                        } else if self.is_connected {
                            ui.colored_label(egui::Color32::GREEN, "● Connected");
                        } else {
                            ui.colored_label(egui::Color32::RED, "● Disconnected");
                        }
                    });
                    ui.label(&self.port_status);
                    ui.horizontal(|ui| {
                        ui.label("Профиль:");
                        let mut profile = self.data_processor.profile();
                        egui::ComboBox::from_id_source("rcm_profile")
                            .selected_text(match profile {
                                RcmProfile::Rcm => "RCM",
                                RcmProfile::Rcms => "RCMS",
                                RcmProfile::Calibration => "Calibration",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut profile, RcmProfile::Rcm, "RCM");
                                ui.selectable_value(&mut profile, RcmProfile::Rcms, "RCMS");
                                ui.selectable_value(&mut profile, RcmProfile::Calibration, "Calibration");
                            });
                        if profile != self.data_processor.profile() {
                            self.data_processor.set_profile(profile);
                            if let Some(live) = self.live.as_ref() {
                                live.set_profile(profile);
                            }
                            self.plot_manager.reset_y_bounds();
                        }

                        let mut filters_on = self.data_processor.filters_enabled();
                        if ui.checkbox(&mut filters_on, "Фильтры").changed() {
                            self.data_processor.set_filters_enabled(filters_on);
                            if let Some(live) = self.live.as_ref() {
                                live.set_filters(filters_on);
                            }
                            self.plot_manager.reset_y_bounds();
                        }
                        if filters_on {
                            ui.weak("(DSP offline, BASE FIR ~1.7s)");
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