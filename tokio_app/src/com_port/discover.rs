//! Автопоиск COM-порта: только 7N1 (JavaFX), короткий probe.

use super::serial::SerialConfig;
use super::sync::FrameSynchronizer;
use std::time::{Duration, Instant};

const MIN_FRAMES: usize = 5;
const PROBE_DURATION: Duration = Duration::from_millis(350);
const SETTLE_DURATION: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub port_name: String,
    pub frames: usize,
    pub resyncs: u64,
    pub bytes_read: usize,
    pub config: SerialConfig,
}

impl ProbeResult {
    pub fn looks_like_rcm(&self) -> bool {
        self.frames >= MIN_FRAMES && self.bytes_read >= 100
    }

    pub fn score(&self) -> i64 {
        self.frames as i64 * 100 - self.resyncs as i64
            + if self.bytes_read > 200 { 50 } else { 0 }
    }
}

pub fn probe_port(port_name: &str, config: &SerialConfig) -> Result<ProbeResult, String> {
    let mut probe_cfg = config.clone();
    probe_cfg.timeout = Duration::from_millis(10);

    let mut port = probe_cfg
        .open(port_name)
        .map_err(|e| format!("{port_name} [7N1]: не открыть ({e})"))?;

    let settle_deadline = Instant::now() + SETTLE_DURATION;
    let mut trash = [0u8; 512];
    while Instant::now() < settle_deadline {
        let _ = port.read(&mut trash);
    }

    let mut sync = FrameSynchronizer::new();
    let mut frames = 0usize;
    let mut bytes_read = 0usize;
    let mut buf = [0u8; 1024];
    let deadline = Instant::now() + PROBE_DURATION;

    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(n) if n > 0 => {
                bytes_read += n;
                frames += sync.push_bytes(&buf[..n]).len();
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(format!("{port_name}: ошибка чтения ({e})")),
        }
    }

    drop(port);

    Ok(ProbeResult {
        port_name: port_name.to_string(),
        frames,
        resyncs: sync.resync_count,
        bytes_read,
        config: config.clone(),
    })
}

pub fn find_rcm_port(baud_rate: u32) -> Option<ProbeResult> {
    find_rcm_port_among(list_port_names(), baud_rate)
}

pub fn find_rcm_port_among(ports: Vec<String>, baud_rate: u32) -> Option<ProbeResult> {
    let cfg = SerialConfig::rcm_7n1(baud_rate);
    let mut best: Option<ProbeResult> = None;
    let mut best_any: Option<ProbeResult> = None;

    for name in &ports {
        match probe_port(name, &cfg) {
            Ok(result) => {
                eprintln!(
                    "probe {name} [7N1]: frames={}, resyncs={}, bytes={}, score={}",
                    result.frames,
                    result.resyncs,
                    result.bytes_read,
                    result.score()
                );
                let better_any = best_any
                    .as_ref()
                    .map(|b| result.score() > b.score())
                    .unwrap_or(true);
                if better_any {
                    best_any = Some(result.clone());
                }
                if result.looks_like_rcm() {
                    let better = best
                        .as_ref()
                        .map(|b| result.score() > b.score())
                        .unwrap_or(true);
                    if better {
                        best = Some(result);
                    }
                }
            }
            Err(e) => eprintln!("probe skip: {e}"),
        }
    }

    best.or_else(|| best_any.filter(|r| r.bytes_read >= 50 || r.frames >= 1))
}

pub fn list_port_names() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| {
            let mut names: Vec<_> = ports.into_iter().map(|p| p.port_name).collect();
            names.sort();
            names
        })
        .unwrap_or_default()
}
