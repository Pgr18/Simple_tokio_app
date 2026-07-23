use super::decode::decode_frame;
use super::discover::find_rcm_port;
use super::model::Frame;
use super::serial::SerialConfig;
use super::sync::FrameSynchronizer;
use serialport::SerialPort;
use std::collections::VecDeque;

pub struct ComPortReader {
    port: Option<Box<dyn SerialPort>>,
    config: SerialConfig,
    sync: FrameSynchronizer,
    /// FIFO сырых кадров — порядок отображения = порядку приёма.
    pending: VecDeque<[u8; 20]>,
}

impl ComPortReader {
    pub fn new() -> Self {
        Self {
            port: None,
            config: SerialConfig::default(),
            sync: FrameSynchronizer::new(),
            pending: VecDeque::new(),
        }
    }

    pub fn with_config(config: SerialConfig) -> Self {
        Self {
            port: None,
            config,
            sync: FrameSynchronizer::new(),
            pending: VecDeque::new(),
        }
    }

    pub fn available_ports() -> Vec<String> {
        super::discover::list_port_names()
    }

    pub fn config(&self) -> &SerialConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: SerialConfig) {
        self.config = config;
    }

    pub fn connect(&mut self, port_name: &str, baud_rate: u32) -> Result<(), Box<dyn std::error::Error>> {
        let mut cfg = self.config.clone();
        cfg.baud_rate = baud_rate;
        self.connect_with_config(port_name, cfg)
    }

    pub fn connect_with_config(
        &mut self,
        port_name: &str,
        config: SerialConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut port = config.open(port_name)?;
        // Сброс мусора после открытия.
        let mut trash = [0u8; 512];
        for _ in 0..8 {
            match port.read(&mut trash) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        self.port = Some(port);
        self.config = config;
        self.clear_buffer();
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.port = None;
        self.clear_buffer();
    }

    pub fn is_connected(&self) -> bool {
        self.port.is_some()
    }

    pub fn clear_buffer(&mut self) {
        self.sync.clear();
        self.pending.clear();
    }

    pub fn buffer_size(&self) -> usize {
        self.sync.buffer_len() + self.pending.len() * 20
    }

    pub fn pending_frames(&self) -> usize {
        self.pending.len()
    }

    pub fn resync_count(&self) -> u64 {
        self.sync.resync_count
    }

    /// Дочитывает байты с порта в синхронизатор (без выдачи кадра).
    pub fn poll_input(&mut self) {
        if self.port.is_none() {
            return;
        }
        let mut temp = [0u8; 1024];
        // Несколько чтений за тик, чтобы не копить джиттер на 38400.
        for _ in 0..4 {
            let read_result = match self.port.as_mut() {
                Some(p) => p.read(&mut temp),
                None => return,
            };
            match read_result {
                Ok(n) if n > 0 => {
                    let frames = self.sync.push_bytes(&temp[..n]);
                    self.pending.extend(frames);
                    // Большой запас: лучше догнать, чем терять секунды записи.
                    while self.pending.len() > 20_000 {
                        self.pending.pop_front();
                    }
                }
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(_) => {
                    self.disconnect();
                    return;
                }
            }
        }
    }

    /// Берёт следующий кадр из FIFO (после `poll_input`).
    pub fn next_raw_frame(&mut self) -> Option<[u8; 20]> {
        self.pending.pop_front()
    }

    /// Читает сырой 20-байтный кадр.
    pub fn read_raw_frame(&mut self) -> Option<[u8; 20]> {
        self.poll_input();
        self.next_raw_frame()
    }

    pub fn read_data(&mut self) -> Option<Frame> {
        self.read_raw_frame().map(|raw| decode_frame(&raw))
    }

    pub fn auto_connect(&mut self, baud_rate: u32) -> Result<String, String> {
        if self.is_connected() {
            self.disconnect();
        }

        let found = find_rcm_port(baud_rate).ok_or_else(|| {
            "Прибор РКМ/РКМ-С не найден ни на одном COM-порту".to_string()
        })?;

        self.connect_with_config(&found.port_name, found.config)
            .map_err(|e| e.to_string())?;
        Ok(found.port_name)
    }
}

impl Default for ComPortReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Прогоняет сырой дамп через синхронизатор + декодер (офлайн).
pub fn decode_dump(bytes: &[u8]) -> DumpStats {
    use super::decode::decode_frame as dec;
    let mut sync = FrameSynchronizer::new();
    let mut frames_raw = sync.push_bytes(bytes);
    if let Some(last) = sync.flush() {
        frames_raw.push(last);
    }

    let mut stats = DumpStats {
        frames: frames_raw.len(),
        resyncs: sync.resync_count,
        remaining_bytes: sync.buffer_len(),
        ..DumpStats::default()
    };

    for raw in &frames_raw {
        let f = dec(raw);
        stats.update(f);
    }
    stats
}

#[derive(Debug, Default, Clone)]
pub struct DumpStats {
    pub frames: usize,
    pub resyncs: u64,
    pub remaining_bytes: usize,
    pub rheo1_min: i32,
    pub rheo1_max: i32,
    pub rheo1_sum: i64,
    pub base1_min: i32,
    pub base1_max: i32,
    pub base1_sum: i64,
    pub ecg_min: i32,
    pub ecg_max: i32,
    pub ecg_sum: i64,
    pub base2_min: i32,
    pub base2_max: i32,
    pub base2_sum: i64,
    pub rheo2_min: i32,
    pub rheo2_max: i32,
    pub rheo2_sum: i64,
    initialized: bool,
}

impl DumpStats {
    fn update(&mut self, f: Frame) {
        if !self.initialized {
            self.rheo1_min = f.rheo1;
            self.rheo1_max = f.rheo1;
            self.base1_min = f.base1;
            self.base1_max = f.base1;
            self.ecg_min = f.ecg;
            self.ecg_max = f.ecg;
            self.base2_min = f.base2;
            self.base2_max = f.base2;
            self.rheo2_min = f.rheo2;
            self.rheo2_max = f.rheo2;
            self.initialized = true;
        } else {
            self.rheo1_min = self.rheo1_min.min(f.rheo1);
            self.rheo1_max = self.rheo1_max.max(f.rheo1);
            self.base1_min = self.base1_min.min(f.base1);
            self.base1_max = self.base1_max.max(f.base1);
            self.ecg_min = self.ecg_min.min(f.ecg);
            self.ecg_max = self.ecg_max.max(f.ecg);
            self.base2_min = self.base2_min.min(f.base2);
            self.base2_max = self.base2_max.max(f.base2);
            self.rheo2_min = self.rheo2_min.min(f.rheo2);
            self.rheo2_max = self.rheo2_max.max(f.rheo2);
        }
        self.rheo1_sum += f.rheo1 as i64;
        self.base1_sum += f.base1 as i64;
        self.ecg_sum += f.ecg as i64;
        self.base2_sum += f.base2 as i64;
        self.rheo2_sum += f.rheo2 as i64;
    }

    pub fn mean(&self, sum: i64) -> f64 {
        if self.frames == 0 {
            0.0
        } else {
            sum as f64 / self.frames as f64
        }
    }
}
