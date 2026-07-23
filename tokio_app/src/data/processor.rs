use crate::com_port::DataPacket;
use crate::data::live_worker::LivePoint;
use crate::data::rcm_pipeline::{
    ChannelFilterFlags, RcmOutSample, RcmPipeline, RcmProfile, SAMPLE_RATE_HZ,
};
use chrono::Local;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct RecordedPacket {
    pub timestamp: u64,
    pub time_seconds: f64,
    pub rheo1: f64,
    pub base1: f64,
    pub ecg: f64,
    pub base2: f64,
    pub rheo2: f64,
    pub qs1: f64,
    pub qs2: f64,
}

pub struct DataProcessor {
    max_points: usize,
    rheo1: Vec<(f64, f64)>,
    base1: Vec<(f64, f64)>,
    ecg: Vec<(f64, f64)>,
    base2: Vec<(f64, f64)>,
    rheo2: Vec<(f64, f64)>,
    qs1: Vec<(f64, f64)>,
    qs2: Vec<(f64, f64)>,
    recorded_data: Vec<RecordedPacket>,
    session_start_time: Option<u64>,
    auto_recording: bool,
    recording_start_datetime: Option<String>,
    session_counter: u32,
    pipeline: RcmPipeline,
    sample_index: u64,
}

impl DataProcessor {
    pub fn new(max_points: usize) -> Self {
        Self::with_profile(max_points, RcmProfile::Rcms)
    }

    pub fn with_profile(max_points: usize, profile: RcmProfile) -> Self {
        let now = Local::now();
        let initial_datetime = Some(now.format("%Y-%m-%d %H-%M-%S").to_string());

        Self {
            max_points,
            rheo1: Vec::with_capacity(max_points),
            base1: Vec::with_capacity(max_points),
            ecg: Vec::with_capacity(max_points),
            base2: Vec::with_capacity(max_points),
            rheo2: Vec::with_capacity(max_points),
            qs1: Vec::with_capacity(max_points),
            qs2: Vec::with_capacity(max_points),
            recorded_data: Vec::new(),
            session_start_time: None,
            auto_recording: true,
            recording_start_datetime: initial_datetime,
            session_counter: 1,
            pipeline: RcmPipeline::new(profile),
            sample_index: 0,
        }
    }

    pub fn set_profile(&mut self, profile: RcmProfile) {
        self.pipeline.set_profile(profile);
        self.sample_index = 0;
    }

    pub fn profile(&self) -> RcmProfile {
        self.pipeline.profile()
    }

    pub fn channel_filters(&self) -> ChannelFilterFlags {
        self.pipeline.flags()
    }

    pub fn set_channel_filters(&mut self, flags: ChannelFilterFlags) {
        if flags != self.pipeline.flags() {
            self.pipeline.set_flags(flags);
            self.clear_channels();
            self.sample_index = 0;
        }
    }

    pub fn set_max_points(&mut self, max_points: usize) {
        self.max_points = max_points.max(1000);
    }

    pub fn max_points(&self) -> usize {
        self.max_points
    }

    /// Точка с фонового DSP-потока (уже в физ. единицах).
    pub fn push_live_point(&mut self, p: &LivePoint) {
        let timestamp = now_ms();
        if self.session_start_time.is_none() {
            self.session_start_time = Some(timestamp);
        }
        if p.time_seconds >= 0.0 {
            let idx = (p.time_seconds * SAMPLE_RATE_HZ).round() as u64 + 1;
            if idx > self.sample_index {
                self.sample_index = idx;
            }
        }

        let max_points = self.max_points;
        Self::push_point(&mut self.rheo1, p.time_seconds, p.rheo1, max_points);
        Self::push_point(&mut self.base1, p.time_seconds, p.base1, max_points);
        Self::push_point(&mut self.ecg, p.time_seconds, p.ecg, max_points);
        Self::push_point(&mut self.base2, p.time_seconds, p.base2, max_points);
        Self::push_point(&mut self.rheo2, p.time_seconds, p.rheo2, max_points);
        Self::push_point(&mut self.qs1, p.time_seconds, p.qs1, max_points);
        Self::push_point(&mut self.qs2, p.time_seconds, p.qs2, max_points);

        if self.auto_recording {
            self.recorded_data.push(RecordedPacket {
                timestamp,
                time_seconds: p.time_seconds,
                rheo1: p.rheo1,
                base1: p.base1,
                ecg: p.ecg,
                base2: p.base2,
                rheo2: p.rheo2,
                qs1: p.qs1,
                qs2: p.qs2,
            });
        }
    }

    /// Сырой 20-байтный кадр → фильтры → буферы графиков.
    /// Ось времени = число *принятых* кадров / 200 Гц (не число выходов фильтра).
    pub fn add_raw_frame(&mut self, raw: &[u8; 20]) {
        let timestamp = now_ms();
        if self.session_start_time.is_none() {
            self.session_start_time = Some(timestamp);
        }

        for sample in self.pipeline.process_raw(raw) {
            let time_seconds = self.sample_index as f64 / SAMPLE_RATE_HZ;
            self.sample_index += 1;
            self.push_filtered_at(sample, timestamp, time_seconds);
        }
    }

    fn push_filtered_at(&mut self, sample: RcmOutSample, timestamp: u64, time_seconds: f64) {
        // Java `Option.INVERSE` на РЕО — инверсия для отображения.
        let rheo1 = -(sample.rheo1 as f64);
        let rheo2 = -(sample.rheo2 as f64);
        let base1 = sample.base1 as f64;
        let base2 = sample.base2 as f64;
        let ecg = sample.ecg as f64;
        let qs1 = sample.qs1 as f64;
        let qs2 = sample.qs2 as f64;

        let max_points = self.max_points;
        Self::push_point(&mut self.rheo1, time_seconds, rheo1, max_points);
        Self::push_point(&mut self.base1, time_seconds, base1, max_points);
        Self::push_point(&mut self.ecg, time_seconds, ecg, max_points);
        Self::push_point(&mut self.base2, time_seconds, base2, max_points);
        Self::push_point(&mut self.rheo2, time_seconds, rheo2, max_points);
        Self::push_point(&mut self.qs1, time_seconds, qs1, max_points);
        Self::push_point(&mut self.qs2, time_seconds, qs2, max_points);

        if self.auto_recording {
            self.recorded_data.push(RecordedPacket {
                timestamp,
                time_seconds,
                rheo1,
                base1,
                ecg,
                base2,
                rheo2,
                qs1,
                qs2,
            });
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
                qs1: 0.0,
                qs2: 0.0,
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
        // Всегда по счётчику принятых кадров — окно едет даже в прогреве фильтров.
        self.sample_index as f64 / SAMPLE_RATE_HZ
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

    pub fn get_qs1(&self) -> &[(f64, f64)] {
        &self.qs1
    }

    pub fn get_qs2(&self) -> &[(f64, f64)] {
        &self.qs2
    }

    pub fn save_to_csv<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        if self.recorded_data.is_empty() {
            return Err("No recorded data to save".into());
        }

        let mut wtr = csv::Writer::from_path(path)?;
        wtr.write_record([
            "time_seconds",
            "rheo1_uOhm",
            "base1_mOhm",
            "ecg_mV",
            "base2_mOhm",
            "rheo2_uOhm",
            "qs1_Ohm",
            "qs2_Ohm",
        ])?;

        for packet in &self.recorded_data {
            wtr.write_record([
                packet.time_seconds.to_string(),
                packet.rheo1.to_string(),
                packet.base1.to_string(),
                packet.ecg.to_string(),
                packet.base2.to_string(),
                packet.rheo2.to_string(),
                packet.qs1.to_string(),
                packet.qs2.to_string(),
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
        self.sample_index = 0;
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
        self.sample_index = 0;
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
        self.qs1.clear();
        self.qs2.clear();
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
