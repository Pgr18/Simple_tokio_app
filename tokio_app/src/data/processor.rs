use crate::com_port::DataPacket;
use std::path::Path;
use chrono::{Local, DateTime};

#[derive(Debug, Clone)]
pub struct RecordedPacket {
    pub timestamp: u64,    // Оригинальный timestamp (могут понадобиться для отладки)
    pub time_seconds: f64, // Время в секундах относительно начала записи
    pub channel_a: f64,
    pub channel_b: f64,
    pub channel_c: f64,
    pub status: u8,
}

pub struct DataProcessor {
    max_points: usize,
    channel_a: Vec<(f64, f64)>, // (time_seconds, value)
    channel_b: Vec<(f64, f64)>, // (time_seconds, value)
    channel_c: Vec<(f64, f64)>, // (time_seconds, value)
    recorded_data: Vec<RecordedPacket>,
    session_start_time: Option<u64>,
    auto_recording: bool,
    recording_start_datetime: Option<String>, // Время начала текущей сессии записи в формате строки
    session_counter: u32, // Счетчик сессий для нумерации файлов
}

impl DataProcessor {
    pub fn new(max_points: usize) -> Self {
        let now = Local::now();
        let initial_datetime = Some(now.format("%Y-%m-%d %H-%M-%S").to_string());
        
        Self {
            max_points,
            channel_a: Vec::with_capacity(max_points),
            channel_b: Vec::with_capacity(max_points),
            channel_c: Vec::with_capacity(max_points),
            recorded_data: Vec::new(),
            session_start_time: None,
            auto_recording: true,
            recording_start_datetime: initial_datetime,
            session_counter: 1, // Начинаем с 1
        }
    }

    pub fn add_packet(&mut self, packet: DataPacket) {
        // Если это первый пакет в сессии, запоминаем время начала
        if self.session_start_time.is_none() {
            self.session_start_time = Some(packet.timestamp);
        }

        // Вычисляем время в секундах относительно начала сессии
        let time_seconds = if let Some(start_time) = self.session_start_time {
            (packet.timestamp - start_time) as f64 / 1000.0 // Преобразуем мс в секунды
        } else {
            0.0
        };

        // Добавляем в каналы отображения
        self.add_to_channel_a(time_seconds, packet.channel_a);
        self.add_to_channel_b(time_seconds, packet.channel_b);
        self.add_to_channel_c(time_seconds, packet.channel_c);

        // Записываем данные только если автозапись включена
        if self.auto_recording {
            self.recorded_data.push(RecordedPacket {
                timestamp: packet.timestamp,
                time_seconds,
                channel_a: packet.channel_a,
                channel_b: packet.channel_b,
                channel_c: packet.channel_c,
                status: packet.status,
            });
        }
    }

    fn add_to_channel_a(&mut self, time_seconds: f64, value: f64) {
        self.channel_a.push((time_seconds, value));
        // Увеличиваем размер буфера чтобы покрыть максимальный масштаб 60s с запасом
        if self.channel_a.len() > self.max_points {
            self.channel_a.remove(0);
        }
    }

    fn add_to_channel_b(&mut self, time_seconds: f64, value: f64) {
        self.channel_b.push((time_seconds, value));
        if self.channel_b.len() > self.max_points {
            self.channel_b.remove(0);
        }
    }

    fn add_to_channel_c(&mut self, time_seconds: f64, value: f64) {
        self.channel_c.push((time_seconds, value));
        if self.channel_c.len() > self.max_points {
            self.channel_c.remove(0);
        }
    }

    /// Получить максимальное время в данных
    pub fn get_max_time(&self) -> f64 {
        let max_a = self.channel_a.last().map(|(t, _)| *t).unwrap_or(0.0);
        let max_b = self.channel_b.last().map(|(t, _)| *t).unwrap_or(0.0);
        let max_c = self.channel_c.last().map(|(t, _)| *t).unwrap_or(0.0);
        max_a.max(max_b).max(max_c)
    }

    /// Получить последнее время в канале A (для синхронизации комбинированного графика)
    pub fn get_last_time(&self) -> f64 {
        self.channel_a.last().map(|(t, _)| *t).unwrap_or(0.0)
    }

    // ... остальные методы без изменений ...
    /// Включить автозапись
    pub fn start_auto_recording(&mut self) {
        if !self.auto_recording {
            self.auto_recording = true;
            // При включении записи обновляем время начала сессии
            let now = Local::now();
            self.recording_start_datetime = Some(now.format("%Y-%m-%d %H-%M-%S").to_string());
        }
    }

    /// Выключить автозапись
    pub fn stop_auto_recording(&mut self) {
        self.auto_recording = false;
    }

    /// Получить статус автозаписи
    pub fn is_auto_recording(&self) -> bool {
        self.auto_recording
    }

    /// Получить время начала записи в формате строки
    pub fn get_recording_start_datetime(&self) -> Option<&str> {
        self.recording_start_datetime.as_deref()
    }

    /// Получить номер текущей сессии
    pub fn get_session_counter(&self) -> u32 {
        self.session_counter
    }

    /// Обновить время начала записи на текущее время
    pub fn update_recording_start_time(&mut self) {
        let now = Local::now();
        self.recording_start_datetime = Some(now.format("%Y-%m-%d %H-%M-%S").to_string());
    }

    /// Сброс записи и начало новой сессии (полная очистка)
    pub fn reset_recording(&mut self) {
        // Полная очистка всех данных
        self.channel_a.clear();
        self.channel_b.clear();
        self.channel_c.clear();
        self.recorded_data.clear();
        self.session_start_time = None;
        // Обновляем время начала на текущее при сбросе
        self.update_recording_start_time();
        // Увеличиваем счетчик сессий
        self.session_counter += 1;
    }

    /// Получить время начала текущей сессии записи
    pub fn get_session_start_time(&self) -> Option<u64> {
        self.session_start_time
    }

    pub fn get_recorded_count(&self) -> usize {
        self.recorded_data.len()
    }

    pub fn save_to_csv<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        if self.recorded_data.is_empty() {
            return Err("No recorded data to save".into());
        }

        let mut wtr = csv::Writer::from_path(path)?;
        
        // Записываем заголовок с временем в секундах
        wtr.write_record(&["time_seconds", "channel_a", "channel_b", "channel_c", "status"])?;

        // Записываем данные
        for packet in &self.recorded_data {
            wtr.write_record(&[
                packet.time_seconds.to_string(),
                packet.channel_a.to_string(),
                packet.channel_b.to_string(),
                packet.channel_c.to_string(),
                packet.status.to_string(),
            ])?;
        }

        wtr.flush()?;
        Ok(())
    }

    pub fn get_channel_a(&self) -> &[(f64, f64)] {
        &self.channel_a
    }

    pub fn get_channel_b(&self) -> &[(f64, f64)] {
        &self.channel_b
    }

    pub fn get_channel_c(&self) -> &[(f64, f64)] {
        &self.channel_c
    }

    pub fn clear(&mut self) {
        self.channel_a.clear();
        self.channel_b.clear();
        self.channel_c.clear();
        self.recorded_data.clear();
        self.session_start_time = None;
        self.auto_recording = true;
        self.update_recording_start_time();
        self.session_counter = 1; // Сбрасываем счетчик
    }
}