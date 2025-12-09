use crate::com_port::DataPacket;
use std::path::Path;
use chrono::{Local, DateTime};

#[derive(Debug, Clone)]
pub struct RecordedPacket {
    pub timestamp: u64,           // Оригинальный timestamp
    pub time_seconds: f64,        // Время в секундах относительно начала записи
    pub rheocardiogram: f64,      // Риккардиограмма
    pub base_impedance: f64,      // Базовая импеданс
    pub ecg: f64,                 // ECG
    pub channel4: f64,            // Зарезервированный канал 4
    pub channel5: f64,            // Зарезервированный канал 5
    pub channel6: f64,            // Зарезервированный канал 6
    pub channel7: f64,            // Зарезервированный канал 7
    pub channel8: f64,            // Зарезервированный канал 8
}

pub struct DataProcessor {
    max_points: usize,
    rheocardiogram: Vec<(f64, f64)>,  // (time_seconds, value)
    base_impedance: Vec<(f64, f64)>,  // (time_seconds, value)
    ecg: Vec<(f64, f64)>,             // (time_seconds, value)
    channel4: Vec<(f64, f64)>,        // (time_seconds, value)
    channel5: Vec<(f64, f64)>,        // (time_seconds, value)
    channel6: Vec<(f64, f64)>,        // (time_seconds, value)
    channel7: Vec<(f64, f64)>,        // (time_seconds, value)
    channel8: Vec<(f64, f64)>,        // (time_seconds, value)
    recorded_data: Vec<RecordedPacket>,
    session_start_time: Option<u64>,
    auto_recording: bool,
    recording_start_datetime: Option<String>,
    session_counter: u32,
}

impl DataProcessor {
    pub fn new(max_points: usize) -> Self {
        let now = Local::now();
        let initial_datetime = Some(now.format("%Y-%m-%d %H-%M-%S").to_string());
        
        Self {
            max_points,
            rheocardiogram: Vec::with_capacity(max_points),
            base_impedance: Vec::with_capacity(max_points),
            ecg: Vec::with_capacity(max_points),
            channel4: Vec::with_capacity(max_points),
            channel5: Vec::with_capacity(max_points),
            channel6: Vec::with_capacity(max_points),
            channel7: Vec::with_capacity(max_points),
            channel8: Vec::with_capacity(max_points),
            recorded_data: Vec::new(),
            session_start_time: None,
            auto_recording: true,
            recording_start_datetime: initial_datetime,
            session_counter: 1,
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
        self.add_to_channel_rheo( time_seconds, packet.rheocardiogram as f64);
        self.add_to_channel_base( time_seconds, packet.base_impedance as f64);
        self.add_to_channel_ecg(time_seconds, packet.ecg as f64);
        self.add_to_channel_ch4( time_seconds, packet.channel4 as f64);


        // Записываем данные только если автозапись включена
        if self.auto_recording {
            self.recorded_data.push(RecordedPacket {
                timestamp: packet.timestamp,
                time_seconds,
                rheocardiogram: packet.rheocardiogram as f64,
                base_impedance: packet.base_impedance as f64,
                ecg: packet.ecg as f64,
                channel4: packet.channel4 as f64,
                channel5: packet.channel5 as f64,
                channel6: packet.channel6 as f64,
                channel7: packet.channel7 as f64,
                channel8: packet.channel8 as f64,
            });
        }
    }

    fn add_to_channel_ecg(&mut self, time_seconds: f64, value: f64) {
        self.ecg.push((time_seconds, value));
        // Увеличиваем размер буфера чтобы покрыть максимальный масштаб 60s с запасом
        if self.ecg.len() > self.max_points {
            self.ecg.remove(0);
        }
    }
    
    fn add_to_channel_rheo(&mut self, time_seconds: f64, value: f64) {
        self.rheocardiogram.push((time_seconds, value));
        // Увеличиваем размер буфера чтобы покрыть максимальный масштаб 60s с запасом
        if self.rheocardiogram.len() > self.max_points {
            self.rheocardiogram.remove(0);
        }
    }
    
    fn add_to_channel_base(&mut self, time_seconds: f64, value: f64) {
        self.base_impedance.push((time_seconds, value));
        // Увеличиваем размер буфера чтобы покрыть максимальный масштаб 60s с запасом
        if self.base_impedance.len() > self.max_points {
            self.base_impedance.remove(0);
        }
    }
    
    fn add_to_channel_ch4(&mut self, time_seconds: f64, value: f64) {
        self.channel4.push((time_seconds, value));
        // Увеличиваем размер буфера чтобы покрыть максимальный масштаб 60s с запасом
        if self.channel4.len() > self.max_points {
            self.channel4.remove(0);
        }
    }

    /// Получить максимальное время в данных
    pub fn get_max_time(&self) -> f64 {
        let max_values = [
            self.rheocardiogram.last().map(|(t, _)| *t).unwrap_or(0.0),
            self.base_impedance.last().map(|(t, _)| *t).unwrap_or(0.0),
            self.ecg.last().map(|(t, _)| *t).unwrap_or(0.0),
            self.channel4.last().map(|(t, _)| *t).unwrap_or(0.0),
            self.channel5.last().map(|(t, _)| *t).unwrap_or(0.0),
            self.channel6.last().map(|(t, _)| *t).unwrap_or(0.0),
            self.channel7.last().map(|(t, _)| *t).unwrap_or(0.0),
            self.channel8.last().map(|(t, _)| *t).unwrap_or(0.0),
        ];
        
        max_values.iter().fold(0.0, |max, &val| max.max(val))
    }

    // Методы доступа к каналам
    pub fn get_rheocardiogram(&self) -> &[(f64, f64)] {
        &self.rheocardiogram
    }
    
    pub fn get_base_impedance(&self) -> &[(f64, f64)] {
        &self.base_impedance
    }
    
    pub fn get_ecg(&self) -> &[(f64, f64)] {
        &self.ecg
    }
    
    pub fn get_channel4(&self) -> &[(f64, f64)] {
        &self.channel4
    }
    
    pub fn get_channel5(&self) -> &[(f64, f64)] {
        &self.channel5
    }
    
    pub fn get_channel6(&self) -> &[(f64, f64)] {
        &self.channel6
    }
    
    pub fn get_channel7(&self) -> &[(f64, f64)] {
        &self.channel7
    }
    
    pub fn get_channel8(&self) -> &[(f64, f64)] {
        &self.channel8
    }

    pub fn save_to_csv<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        if self.recorded_data.is_empty() {
            return Err("No recorded data to save".into());
        }

        let mut wtr = csv::Writer::from_path(path)?;
        
        // Записываем заголовок
        wtr.write_record(&[
            "time_seconds", 
            "rheocardiogram", 
            "base_impedance", 
            "ecg",
            "channel4",
            "channel5",
            "channel6",
            "channel7",
            "channel8"
        ])?;

        // Записываем данные
        for packet in &self.recorded_data {
            wtr.write_record(&[
                packet.time_seconds.to_string(),
                packet.rheocardiogram.to_string(),
                packet.base_impedance.to_string(),
                packet.ecg.to_string(),
                packet.channel4.to_string(),
                packet.channel5.to_string(),
                packet.channel6.to_string(),
                packet.channel7.to_string(),
                packet.channel8.to_string(),
            ])?;
        }

        wtr.flush()?;
        Ok(())
    }

    // ... остальные методы (start_auto_recording, stop_auto_recording, etc.)
    pub fn start_auto_recording(&mut self) {
        if !self.auto_recording {
            self.auto_recording = true;
            let now = Local::now();
            self.recording_start_datetime = Some(now.format("%Y-%m-%d %H-%M-%S").to_string());
        }
    }

    pub fn stop_auto_recording(&mut self) {
        self.auto_recording = false;
    }

    pub fn is_auto_recording(&self) -> bool {
        self.auto_recording
    }

    pub fn get_recording_start_datetime(&self) -> Option<&str> {
        self.recording_start_datetime.as_deref()
    }

    pub fn get_session_counter(&self) -> u32 {
        self.session_counter
    }

    pub fn update_recording_start_time(&mut self) {
        let now = Local::now();
        self.recording_start_datetime = Some(now.format("%Y-%m-%d %H-%M-%S").to_string());
    }

    pub fn reset_recording(&mut self) {
        // Полная очистка всех каналов
        self.rheocardiogram.clear();
        self.base_impedance.clear();
        self.ecg.clear();
        self.channel4.clear();
        self.channel5.clear();
        self.channel6.clear();
        self.channel7.clear();
        self.channel8.clear();
        self.recorded_data.clear();
        self.session_start_time = None;
        self.update_recording_start_time();
        self.session_counter += 1;
    }

    pub fn get_session_start_time(&self) -> Option<u64> {
        self.session_start_time
    }

    pub fn get_recorded_count(&self) -> usize {
        self.recorded_data.len()
    }

    pub fn clear(&mut self) {
        self.rheocardiogram.clear();
        self.base_impedance.clear();
        self.ecg.clear();
        self.channel4.clear();
        self.channel5.clear();
        self.channel6.clear();
        self.channel7.clear();
        self.channel8.clear();
        self.recorded_data.clear();
        self.session_start_time = None;
        self.auto_recording = true;
        self.update_recording_start_time();
        self.session_counter = 1;
    }
}