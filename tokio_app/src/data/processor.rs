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
    recording: bool,
    recorded_data: Vec<RecordedPacket>,
}

impl DataProcessor {
    pub fn new(max_points: usize) -> Self {
        Self {
            max_points,
            channel_a: Vec::with_capacity(max_points),
            channel_b: Vec::with_capacity(max_points),
            channel_c: Vec::with_capacity(max_points),
            recording: false,
            recorded_data: Vec::new(),
        }
    }

    pub fn add_packet(&mut self, packet: DataPacket) {
        let DataPacket {
            timestamp,
            channel_a,
            channel_b,
            channel_c,
            status,
        } = packet;

        // Обрабатываем каждый канал отдельно
        self.process_channel_a(timestamp, channel_a);
        self.process_channel_b(timestamp, channel_b);
        self.process_channel_c(timestamp, channel_c);

        // Если запись активна, сохраняем полные данные
        if self.recording {
            self.recorded_data.push(RecordedPacket {
                timestamp,
                channel_a,
                channel_b,
                channel_c,
                status,
            });
        }
    }

    fn process_channel_a(&mut self, timestamp: u64, value: f64) {
        self.channel_a.push((timestamp, value));
        if self.channel_a.len() > self.max_points {
            self.channel_a.remove(0);
        }
    }

    fn process_channel_b(&mut self, timestamp: u64, value: f64) {
        self.channel_b.push((timestamp, value));
        if self.channel_b.len() > self.max_points {
            self.channel_b.remove(0);
        }
    }

    fn process_channel_c(&mut self, timestamp: u64, value: f64) {
        self.channel_c.push((timestamp, value));
        if self.channel_c.len() > self.max_points {
            self.channel_c.remove(0);
        }
    }

    pub fn start_recording(&mut self) {
        self.recording = true;
        self.recorded_data.clear();
    }

    pub fn stop_recording(&mut self) {
        self.recording = false;
    }

    pub fn is_recording(&self) -> bool {
        self.recording
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

    pub fn clear_recorded_data(&mut self) {
        self.recorded_data.clear();
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
        self.recording = false;
    }
}