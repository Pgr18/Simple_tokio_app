use crate::com_port::DataPacket;
use chrono::Local;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RecordedPacket {
    pub timestamp: u64,
    pub time_seconds: f64,
    pub rheo1: f64,
    pub base1: f64,
    pub ecg: f64,
    pub base2: f64,
    pub rheo2: f64,
}

pub struct DataProcessor {
    max_points: usize,
    rheo1: Vec<(f64, f64)>,
    base1: Vec<(f64, f64)>,
    ecg: Vec<(f64, f64)>,
    base2: Vec<(f64, f64)>,
    rheo2: Vec<(f64, f64)>,
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
            rheo1: Vec::with_capacity(max_points),
            base1: Vec::with_capacity(max_points),
            ecg: Vec::with_capacity(max_points),
            base2: Vec::with_capacity(max_points),
            rheo2: Vec::with_capacity(max_points),
            recorded_data: Vec::new(),
            session_start_time: None,
            auto_recording: true,
            recording_start_datetime: initial_datetime,
            session_counter: 1,
        }
    }

    pub fn add_packet(&mut self, packet: DataPacket) {
        if self.session_start_time.is_none() {
            self.session_start_time = Some(packet.timestamp);
        }

        let time_seconds = if let Some(start_time) = self.session_start_time {
            (packet.timestamp - start_time) as f64 / 1000.0
        } else {
            0.0
        };

        let max_points = self.max_points;
        Self::push_point(&mut self.rheo1, time_seconds, packet.rheo1 as f64, max_points);
        Self::push_point(&mut self.base1, time_seconds, packet.base1 as f64, max_points);
        Self::push_point(&mut self.ecg, time_seconds, packet.ecg as f64, max_points);
        Self::push_point(&mut self.base2, time_seconds, packet.base2 as f64, max_points);
        Self::push_point(&mut self.rheo2, time_seconds, packet.rheo2 as f64, max_points);

        if self.auto_recording {
            self.recorded_data.push(RecordedPacket {
                timestamp: packet.timestamp,
                time_seconds,
                rheo1: packet.rheo1 as f64,
                base1: packet.base1 as f64,
                ecg: packet.ecg as f64,
                base2: packet.base2 as f64,
                rheo2: packet.rheo2 as f64,
            });
        }
    }

    fn push_point(buf: &mut Vec<(f64, f64)>, time_seconds: f64, value: f64, max_points: usize) {
        buf.push((time_seconds, value));
        if buf.len() > max_points {
            buf.remove(0);
        }
    }

    pub fn get_max_time(&self) -> f64 {
        [
            self.rheo1.last(),
            self.base1.last(),
            self.ecg.last(),
            self.base2.last(),
            self.rheo2.last(),
        ]
        .into_iter()
        .flatten()
        .map(|(t, _)| *t)
        .fold(0.0, f64::max)
    }

    pub fn get_rheocardiogram(&self) -> &[(f64, f64)] {
        &self.rheo1
    }

    pub fn get_base_impedance(&self) -> &[(f64, f64)] {
        &self.base1
    }

    pub fn get_ecg(&self) -> &[(f64, f64)] {
        &self.ecg
    }

    pub fn get_channel4(&self) -> &[(f64, f64)] {
        &self.rheo2
    }

    pub fn get_base2(&self) -> &[(f64, f64)] {
        &self.base2
    }

    pub fn get_rheo2(&self) -> &[(f64, f64)] {
        &self.rheo2
    }

    pub fn save_to_csv<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        if self.recorded_data.is_empty() {
            return Err("No recorded data to save".into());
        }

        let mut wtr = csv::Writer::from_path(path)?;
        wtr.write_record([
            "time_seconds",
            "rheo1",
            "base1",
            "ecg",
            "base2",
            "rheo2",
        ])?;

        for packet in &self.recorded_data {
            wtr.write_record([
                packet.time_seconds.to_string(),
                packet.rheo1.to_string(),
                packet.base1.to_string(),
                packet.ecg.to_string(),
                packet.base2.to_string(),
                packet.rheo2.to_string(),
            ])?;
        }

        wtr.flush()?;
        Ok(())
    }

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
        self.clear_channels();
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
        self.clear_channels();
        self.recorded_data.clear();
        self.session_start_time = None;
        self.auto_recording = true;
        self.update_recording_start_time();
        self.session_counter = 1;
    }

    fn clear_channels(&mut self) {
        self.rheo1.clear();
        self.base1.clear();
        self.ecg.clear();
        self.base2.clear();
        self.rheo2.clear();
    }
}
