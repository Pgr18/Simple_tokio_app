//! Автопоиск COM-порта с прибором РКМ / РКМ-С по валидным кадрам протокола.

use super::serial::SerialConfig;
use super::sync::FrameSynchronizer;
use std::time::{Duration, Instant};

const MIN_FRAMES: usize = 10;
const PROBE_DURATION: Duration = Duration::from_millis(800);
const SETTLE_DURATION: Duration = Duration::from_millis(80);

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub port_name: String,
    pub frames: usize,
    pub resyncs: u64,
    pub bytes_read: usize,
}

impl ProbeResult {
    pub fn looks_like_rcm(&self) -> bool {
        // На живой линии допускаем редкие resync; главное — устойчивый поток кадров.
        self.frames >= MIN_FRAMES
            && self.bytes_read >= 400
            && (self.resyncs as usize) <= self.frames.saturating_mul(3)
    }

    pub fn score(&self) -> i64 {
        self.frames as i64 * 10 - self.resyncs as i64
    }
}

/// Пробует один порт: открывает с конфигом РКМ, читает поток и считает кадры.
pub fn probe_port(port_name: &str, config: &SerialConfig) -> Result<ProbeResult, String> {
    let mut probe_cfg = config.clone();
    probe_cfg.timeout = Duration::from_millis(30);

    let mut port = probe_cfg
        .open(port_name)
        .map_err(|e| format!("{port_name}: не удалось открыть ({e})"))?;

    // Сброс мусора после RTS/DTR.
    let settle_deadline = Instant::now() + SETTLE_DURATION;
    let mut trash = [0u8; 512];
    while Instant::now() < settle_deadline {
        let _ = port.read(&mut trash);
    }

    let mut sync = FrameSynchronizer::new();
    let mut frames = 0usize;
    let mut bytes_read = 0usize;
    let mut buf = [0u8; 512];
    let deadline = Instant::now() + PROBE_DURATION;

    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(n) if n > 0 => {
                bytes_read += n;
                frames += sync.push_bytes(&buf[..n]).len();
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => {
                return Err(format!("{port_name}: ошибка чтения ({e})"));
            }
        }
    }

    drop(port);

    Ok(ProbeResult {
        port_name: port_name.to_string(),
        frames,
        resyncs: sync.resync_count,
        bytes_read,
    })
}

pub fn find_rcm_port(config: &SerialConfig) -> Option<ProbeResult> {
    find_rcm_port_among(list_port_names(), config)
}

pub fn find_rcm_port_among(ports: Vec<String>, config: &SerialConfig) -> Option<ProbeResult> {
    let mut best: Option<ProbeResult> = None;

    for name in ports {
        match probe_port(&name, config) {
            Ok(result) => {
                eprintln!(
                    "probe {name}: frames={}, resyncs={}, bytes={}, score={}",
                    result.frames,
                    result.resyncs,
                    result.bytes_read,
                    result.score()
                );
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

    best
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
