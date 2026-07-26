//! Офлайн-проигрывание `.bin` дампов (как `tests/fixtures/*.bin`).
//!
//! Sync → пайплайн с выбираемыми фильтрами по каналам.

use crate::com_port::sync::FrameSynchronizer;
use crate::data::live_worker::LivePoint;
use crate::data::rcm_pipeline::{
    ChannelFilterFlags, RcmOutSample, RcmPipeline, RcmProfile, SAMPLE_RATE_HZ,
};
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
    pub filters: ChannelFilterFlags,
}

#[derive(Debug, Clone)]
pub struct BinLoadResult {
    pub points: Vec<LivePoint>,
    pub stats: BinLoadStats,
}

pub fn load_bin_file(
    path: &Path,
    profile: RcmProfile,
    flags: ChannelFilterFlags,
) -> Result<BinLoadResult, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("не прочитать {}: {e}", path.display()))?;
    decode_bin_bytes(&bytes, path.to_path_buf(), profile, flags)
}

pub fn decode_bin_bytes(
    bytes: &[u8],
    path: PathBuf,
    profile: RcmProfile,
    flags: ChannelFilterFlags,
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

    let mut pipeline = RcmPipeline::new_with_flags(profile, flags);
    let mut points = Vec::with_capacity(n_frames);
    let mut out_index: u64 = 0;

    for raw in &frames {
        for s in pipeline.process_raw(raw) {
            let t = out_index as f64 / SAMPLE_RATE_HZ;
            out_index += 1;
            points.push(out_to_point(t, &s));
        }
    }

    let samples_out = points.len();
    let duration_s = out_index as f64 / SAMPLE_RATE_HZ;

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
            filters: flags,
        },
    })
}

fn out_to_point(time_seconds: f64, s: &RcmOutSample) -> LivePoint {
    LivePoint {
        time_seconds,
        rheo1: -(s.rheo1 as f64) / 1000.0, // в мОм
        base1: s.base1 as f64,
        ecg: s.ecg as f64 / 1000.0, // в В
        base2: s.base2 as f64,
        rheo2: -(s.rheo2 as f64) / 1000.0, // в мОм
        qs1: s.qs1 as f64,
        qs2: s.qs2 as f64,
    }
}

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
        for seed in 0..400u16 {
            let seed = seed as u8;
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

        let r = decode_bin_bytes(
            &bytes,
            PathBuf::from("synth.bin"),
            RcmProfile::Rcms,
            ChannelFilterFlags::default(),
        )
        .expect("decode");
        assert!(r.stats.frames >= 399, "frames={}", r.stats.frames);
        assert!(!r.points.is_empty());
        assert!(r.stats.duration_s > 1.0);
    }
}
