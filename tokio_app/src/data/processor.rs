use crate::com_port::DataPacket;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RecordedPacket {
    pub timestamp: u64,
    pub channel_a: f64,
    pub channel_b: f64,
    pub channel_c: f64,
    pub status: u8,
}

pub struct DataProcessor {
    max_points: usize,
    channel_a: Vec<(u64, f64)>,
    channel_b: Vec<(u64, f64)>,
    channel_c: Vec<(u64, f64)>,
    recorded_data: Vec<RecordedPacket>,
    session_start_time: Option<u64>,
    auto_recording: bool,
    recording_start_datetime: Option<String>, // Время начала текущей сессии записи в формате строки
}

impl DataProcessor {
    pub fn new(max_points: usize) -> Self {
        Self {
            max_points,
            channel_a: Vec::with_capacity(max_points),
            channel_b: Vec::with_capacity(max_points),
            channel_c: Vec::with_capacity(max_points),
            recorded_data: Vec::new(),
            session_start_time: None,
            auto_recording: true,
            recording_start_datetime: None,
        }
    }

    pub fn add_packet(&mut self, packet: DataPacket) {
        // Если это первый пакет в сессии, запоминаем время начала
        if self.session_start_time.is_none() {
            self.session_start_time = Some(packet.timestamp);
        }

        // Добавляем в каналы отображения
        self.add_to_channel_a(packet.timestamp, packet.channel_a);
        self.add_to_channel_b(packet.timestamp, packet.channel_b);
        self.add_to_channel_c(packet.timestamp, packet.channel_c);

        // Записываем данные только если автозапись включена
        if self.auto_recording {
            self.recorded_data.push(RecordedPacket {
                timestamp: packet.timestamp,
                channel_a: packet.channel_a,
                channel_b: packet.channel_b,
                channel_c: packet.channel_c,
                status: packet.status,
            });
        }
    }

    fn add_to_channel_a(&mut self, timestamp: u64, value: f64) {
        self.channel_a.push((timestamp, value));
        if self.channel_a.len() > self.max_points {
            self.channel_a.remove(0);
        }
    }

    fn add_to_channel_b(&mut self, timestamp: u64, value: f64) {
        self.channel_b.push((timestamp, value));
        if self.channel_b.len() > self.max_points {
            self.channel_b.remove(0);
        }
    }

    fn add_to_channel_c(&mut self, timestamp: u64, value: f64) {
        self.channel_c.push((timestamp, value));
        if self.channel_c.len() > self.max_points {
            self.channel_c.remove(0);
        }
    }

    /// Включить автозапись
    pub fn start_auto_recording(&mut self) {
        if !self.auto_recording {
            self.auto_recording = true;
            // При включении записи обновляем время начала сессии
            let now = chrono::Local::now();
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

    /// Сброс записи и начало новой сессии
    pub fn reset_recording(&mut self) {
        self.recorded_data.clear();
        self.session_start_time = None;
        self.recording_start_datetime = None;
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
        
        // Записываем заголовок
        wtr.write_record(&["timestamp", "channel_a", "channel_b", "channel_c", "status"])?;

        // Записываем данные
        for packet in &self.recorded_data {
            wtr.write_record(&[
                packet.timestamp.to_string(),
                packet.channel_a.to_string(),
                packet.channel_b.to_string(),
                packet.channel_c.to_string(),
                packet.status.to_string(),
            ])?;
        }

        wtr.flush()?;
        Ok(())
    }

    pub fn get_channel_a(&self) -> &[(u64, f64)] {
        &self.channel_a
    }

    pub fn get_channel_b(&self) -> &[(u64, f64)] {
        &self.channel_b
    }

    pub fn get_channel_c(&self) -> &[(u64, f64)] {
        &self.channel_c
    }

    pub fn clear(&mut self) {
        self.channel_a.clear();
        self.channel_b.clear();
        self.channel_c.clear();
        self.recorded_data.clear();
        self.session_start_time = None;
        self.auto_recording = true;
        self.recording_start_datetime = None;
    }
}