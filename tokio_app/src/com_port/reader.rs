use super::decode::decode_frame;
use super::discover::find_rcm_port;
use super::model::Frame;
use super::serial::SerialConfig;
use super::sync::FrameSynchronizer;
use serialport::SerialPort;

pub struct ComPortReader {
    port: Option<Box<dyn SerialPort>>,
    config: SerialConfig,
    sync: FrameSynchronizer,
    pending: Vec<[u8; 20]>,
}

impl ComPortReader {
    pub fn new() -> Self {
        Self {
            port: None,
            config: SerialConfig::default(),
            sync: FrameSynchronizer::new(),
            pending: Vec::new(),
        }
    }

    pub fn with_config(config: SerialConfig) -> Self {
        Self {
            port: None,
            config,
            sync: FrameSynchronizer::new(),
            pending: Vec::new(),
        }
    }

    pub fn available_ports() -> Vec<String> {
        super::discover::list_port_names()
    }

    pub fn connect(&mut self, port_name: &str, baud_rate: u32) -> Result<(), Box<dyn std::error::Error>> {
        let mut cfg = self.config.clone();
        cfg.baud_rate = baud_rate;
        let mut port = cfg.open(port_name)?;
        // Сброс мусора после RTS/DTR.
        let mut trash = [0u8; 512];
        for _ in 0..5 {
            let _ = port.read(&mut trash);
        }
        self.port = Some(port);
        self.config = cfg;
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
        self.sync.buffer_len()
    }

    pub fn resync_count(&self) -> u64 {
        self.sync.resync_count
    }

    /// Читает сырой 20-байтный кадр (для фильтрации / калибровки).
    pub fn read_raw_frame(&mut self) -> Option<[u8; 20]> {
        if let Some(frame) = self.pending.pop() {
            return Some(frame);
        }
        if self.port.is_none() {
            return None;
        }

        let mut temp = [0u8; 256];
        let read_result = self.port.as_mut()?.read(&mut temp);

        match read_result {
            Ok(n) if n > 0 => {
                let frames = self.sync.push_bytes(&temp[..n]);
                if frames.is_empty() {
                    return None;
                }
                let mut iter = frames.into_iter();
                let first = iter.next()?;
                self.pending.extend(iter.rev());
                Some(first)
            }
            Ok(_) => None,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => None,
            Err(_) => {
                self.disconnect();
                None
            }
        }
    }

    /// Декодированный кадр (без выходных фильтров РКМ).
    pub fn read_data(&mut self) -> Option<Frame> {
        self.read_raw_frame().map(|raw| decode_frame(&raw))
    }

    pub fn auto_connect(&mut self, baud_rate: u32) -> Result<String, String> {
        if self.is_connected() {
            self.disconnect();
        }

        let mut cfg = self.config.clone();
        cfg.baud_rate = baud_rate;

        let found = find_rcm_port(&cfg).ok_or_else(|| {
            "Прибор РКМ/РКМ-С не найден ни на одном COM-порту".to_string()
        })?;

        self.connect(&found.port_name, baud_rate)
            .map_err(|e| e.to_string())?;
        Ok(found.port_name)
    }
}

impl Default for ComPortReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Прогоняет сырой дамп через синхронизатор + декодер (офлайн, без async).
pub fn decode_dump(bytes: &[u8]) -> DumpStats {
    use super::decode::decode_frame as dec;
    let mut sync = FrameSynchronizer::new();
    let frames_raw = sync.push_bytes(bytes);

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
