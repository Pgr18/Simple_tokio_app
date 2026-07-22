//! Автопоиск COM-порта с прибором РКМ / РКМ-С по валидным кадрам протокола.

use super::serial::SerialConfig;
use super::sync::FrameSynchronizer;
use std::time::{Duration, Instant};

/// Минимальное число успешно собранных кадров, чтобы считать порт «прибором».
const MIN_FRAMES: usize = 5;

/// Сколько времени слушать каждый порт при пробе.
const PROBE_DURATION: Duration = Duration::from_millis(400);

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub port_name: String,
    pub frames: usize,
    pub resyncs: u64,
    pub bytes_read: usize,
}

impl ProbeResult {
    pub fn looks_like_rcm(&self) -> bool {
        self.frames >= MIN_FRAMES && self.resyncs == 0
    }
}

/// Пробует один порт: открывает с конфигом РКМ, читает поток и считает кадры.
pub fn probe_port(port_name: &str, config: &SerialConfig) -> Result<ProbeResult, String> {
    let mut probe_cfg = config.clone();
    // Короткий timeout, чтобы цикл чтения не зависал на пустом порту.
    probe_cfg.timeout = Duration::from_millis(30);

    let mut port = probe_cfg
        .open(port_name)
        .map_err(|e| format!("{port_name}: не удалось открыть ({e})"))?;

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

/// Перебирает доступные COM-порты и возвращает первый, похожий на РКМ / РКМ-С.
///
/// Критерий: за ~400 мс получено ≥5 валидных кадров без потери синхронизации.
pub fn find_rcm_port(config: &SerialConfig) -> Option<ProbeResult> {
    find_rcm_port_among(list_port_names(), config)
}

/// То же, но по заданному списку имён портов (удобно для UI / тестов).
pub fn find_rcm_port_among(ports: Vec<String>, config: &SerialConfig) -> Option<ProbeResult> {
    let mut best: Option<ProbeResult> = None;

    for name in ports {
        match probe_port(&name, config) {
            Ok(result) => {
                eprintln!(
                    "probe {name}: frames={}, resyncs={}, bytes={}",
                    result.frames, result.resyncs, result.bytes_read
                );
                if result.looks_like_rcm() {
                    if best
                        .as_ref()
                        .map(|b| result.frames > b.frames)
                        .unwrap_or(true)
                    {
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
