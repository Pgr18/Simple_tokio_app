//! Офлайн-проигрывание `.bin` дампов (как `tests/fixtures/*.bin`).
//!
//! Путь: байты → FrameSynchronizer → сырой decode → LivePoint[].
//! Фильтры не применяются — дампы уже отфильтрованы.

use crate::com_port::decode::decode_frame;
use crate::com_port::sync::FrameSynchronizer;
use crate::data::live_worker::LivePoint;
use crate::data::rcm_pipeline::{RcmProfile, SAMPLE_RATE_HZ};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct BinLoadStats {
    pub path: PathBuf,
    pub bytes: usize,
    pub frames: usize,
    pub resyncs: u64,
    pub remaining_bytes: usize,
    pub samples_out: usize,
    pub duration_s: f64,
    pub profile: RcmProfile,
}

#[derive(Debug, Clone)]
pub struct BinLoadResult {
    pub points: Vec<LivePoint>,
    pub stats: BinLoadStats,
}

/// Загружает `.bin` и прогоняет через sync + сырой decode.
/// Фильтры **не** применяются: дампы уже содержат готовый поток
/// (или сырые кадры для проверки парсера), DSP поверх не нужен.
pub fn load_bin_file(path: &Path, profile: RcmProfile) -> Result<BinLoadResult, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("не прочитать {}: {e}", path.display()))?;
    decode_bin_bytes(&bytes, path.to_path_buf(), profile)
}

pub fn decode_bin_bytes(
    bytes: &[u8],
    path: PathBuf,
    profile: RcmProfile,
) -> Result<BinLoadResult, String> {
    let mut sync = if profile == RcmProfile::Rcms {
        FrameSynchronizer::new_rcms()
    } else {
        FrameSynchronizer::new()
    };

    let mut frames = sync.push_bytes(bytes);
    if let Some(last) = sync.flush() {
        frames.push(last);
    }

    let resyncs = sync.resync_count;
    let remaining = sync.buffer_len();
    let n_frames = frames.len();

    let mut points = Vec::with_capacity(n_frames);
    for (i, raw) in frames.iter().enumerate() {
        let t = i as f64 / SAMPLE_RATE_HZ;
        let f = decode_frame(raw);
        points.push(LivePoint {
            time_seconds: t,
            rheo1: f.rheo1 as f64,
            base1: f.base1 as f64,
            ecg: f.ecg as f64,
            base2: f.base2 as f64,
            rheo2: f.rheo2 as f64,
            qs1: 0.0,
            qs2: 0.0,
        });
    }

    let samples_out = points.len();
    let duration_s = samples_out as f64 / SAMPLE_RATE_HZ;

    Ok(BinLoadResult {
        points,
        stats: BinLoadStats {
            path,
            bytes: bytes.len(),
            frames: n_frames,
            resyncs,
            remaining_bytes: remaining,
            samples_out,
            duration_s,
            profile,
        },
    })
}

/// Угадать профиль по имени файла (`*rcms*` → RCMS, иначе RCM).
pub fn guess_profile_from_path(path: &Path) -> RcmProfile {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.contains("rcms") {
        RcmProfile::Rcms
    } else {
        RcmProfile::Rcm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_synthetic_bin() {
        let mut bytes = Vec::new();
        for seed in 0..100u8 {
            let mut f = [0u8; 20];
            for i in 0..10 {
                let high = if i == 0 {
                    (seed & 0x7E) & !1
                } else {
                    ((seed.wrapping_add(i as u8) << 1) & 0x7E) | 0x01
                };
                let low = ((seed.wrapping_add(i as u8 * 3) << 1) & 0x7E) & !1;
                f[i * 2] = high;
                f[i * 2 + 1] = low;
            }
            f[18] = 0x01;
            f[19] = 0x00;
            bytes.extend_from_slice(&f);
        }

        let r = decode_bin_bytes(&bytes, PathBuf::from("synth.bin"), RcmProfile::Rcms)
            .expect("decode");
        assert!(r.stats.frames >= 99, "frames={}", r.stats.frames);
        assert_eq!(r.points.len(), r.stats.frames);
        assert!(r.stats.duration_s > 0.4);
    }
}
