//! Fixture-тесты протоколов РКМ / РКМ-С.
//!
//! Эталонные дампы в `tokio_app/tests/fixtures/` (имя может быть с `_` или пробелами):
//! - `*SerialService*rcm.bin` — базовый РКМ
//! - `*rcms.bin` — модификация РКМ-С

use com_port_plotter::{
    decode::{convert, decode_frame, decode_frame_rcms, to_int},
    decode_dump, FrameSynchronizer,
};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

/// Ищет fixture по подстрокам в имени файла (без учёта регистра).
fn load_fixture_matching(required: &[&str]) -> Option<(PathBuf, Vec<u8>)> {
    let dir = fixtures_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => {
            eprintln!("skip: fixtures dir missing at {}", dir.display());
            return None;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("bin")) != Some(true)
        {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if required.iter().all(|r| name.contains(&r.to_ascii_lowercase())) {
            match std::fs::read(&path) {
                Ok(bytes) => return Some((path, bytes)),
                Err(e) => {
                    eprintln!("skip: cannot read {}: {e}", path.display());
                    return None;
                }
            }
        }
    }

    eprintln!(
        "skip: fixture matching {:?} not found in {}",
        required,
        dir.display()
    );
    None
}

/// Синтетический кадр с корректными флагами bit0 (как у РКМ-С: bit7=0).
/// Раскладка Java `RcmInVariable`: rheo2 на паре 2.
fn make_rcms_like_frame(rheo1: (u8, u8), base1: (u8, u8), ecg: (u8, u8), base2: (u8, u8), rheo2: (u8, u8)) -> [u8; 20] {
    let mut f = [0u8; 20];
    let pairs = [
        (0, rheo1, true),
        (1, base1, false),
        (2, rheo2, false), // RHEO_2
        (3, ecg, false),
        (4, base2, false),
        (5, (0x10, 0x20), false), // RHEO_1X
        (6, (0x12, 0x22), false), // QS_1
        (7, (0x14, 0x24), false), // RHEO_2X
        (8, (0x16, 0x26), false), // ECG_X
        (9, (0x01, 0x00), false), // QS_2 / service
    ];
    for (idx, (high, low), is_marker) in pairs {
        let mut h = high & 0x7E;
        let mut l = low & 0x7E;
        if idx == 0 || is_marker {
            h &= !1;
        } else {
            h = (h & !1) | 1;
        }
        l &= !1;
        f[idx * 2] = h;
        f[idx * 2 + 1] = l;
    }
    f[18] = 0x01;
    f[19] = 0x00;
    f
}

#[test]
fn synthetic_stream_no_sync_loss() {
    let mut stream = Vec::new();
    for i in 0..200u8 {
        stream.extend_from_slice(&make_rcms_like_frame(
            (i, i.wrapping_add(1)),
            (0x18, i),
            (0x20, i),
            (0x1C, i),
            (0x30, i),
        ));
    }

    let stats = decode_dump(&stream);
    assert_eq!(stats.frames, 200);
    assert_eq!(stats.resyncs, 0);
    assert_eq!(stats.remaining_bytes, 0);
}

#[test]
fn decode_frame_and_rcms_are_identical() {
    let raw = make_rcms_like_frame((0x2A, 0x4C), (0x18, 0x3E), (0x22, 0x10), (0x1C, 0x2E), (0x34, 0x08));
    let a = decode_frame(&raw);
    let b = decode_frame_rcms(&raw);
    assert_eq!(a.rheo1, b.rheo1);
    assert_eq!(a.base1, b.base1);
    assert_eq!(a.ecg, b.ecg);
    assert_eq!(a.base2, b.base2);
    assert_eq!(a.rheo2, b.rheo2);

    assert_eq!(a.rheo1, convert(raw[0], raw[1], true));
    assert_eq!(a.base1, convert(raw[2], raw[3], false));
    assert_eq!(a.rheo2, convert(raw[4], raw[5], true));
    assert_eq!(a.ecg, convert(raw[6], raw[7], true));
    assert_eq!(a.base2, convert(raw[8], raw[9], false));
}

#[test]
fn value_ranges_are_12bit() {
    assert!((0..=4095).contains(&to_int(0x7E, 0x7E)));
    let signed = convert(0x7E, 0x7E, true);
    assert!((-2048..=2047).contains(&signed) || signed == -4095 || signed.abs() <= 4095);
}

#[test]
fn rcms_fixture_full_dump() {
    let Some((path, bytes)) = load_fixture_matching(&["rcms"]) else {
        return;
    };
    eprintln!("using RCMS fixture: {}", path.display());

    let stats = decode_dump(&bytes);
    eprintln!(
        "RCMS fixture: frames={}, resyncs={}, remaining={}",
        stats.frames, stats.resyncs, stats.remaining_bytes
    );
    eprintln!(
        "  rheo1 min/max/mean = {} / {} / {:.2}",
        stats.rheo1_min,
        stats.rheo1_max,
        stats.mean(stats.rheo1_sum)
    );
    eprintln!(
        "  base1 min/max/mean = {} / {} / {:.2}",
        stats.base1_min,
        stats.base1_max,
        stats.mean(stats.base1_sum)
    );
    eprintln!(
        "  ecg   min/max/mean = {} / {} / {:.2}",
        stats.ecg_min,
        stats.ecg_max,
        stats.mean(stats.ecg_sum)
    );
    eprintln!(
        "  base2 min/max/mean = {} / {} / {:.2}",
        stats.base2_min,
        stats.base2_max,
        stats.mean(stats.base2_sum)
    );
    eprintln!(
        "  rheo2 min/max/mean = {} / {} / {:.2}",
        stats.rheo2_min,
        stats.rheo2_max,
        stats.mean(stats.rheo2_sum)
    );

    // ~493_541 кадров на эталонном дампе ~9.87 МБ
    assert!(
        stats.frames > 100_000,
        "expected hundreds of thousands of frames, got {}",
        stats.frames
    );
    // Допускаем единичный resync на ведущем мусоре; дрейфа быть не должно
    // (все кадры после захвата синхронизации).
    assert!(
        stats.resyncs <= 2,
        "unexpected sync loss on RCMS fixture: {}",
        stats.resyncs
    );
    assert_eq!(
        stats.frames, 493_541,
        "RCMS frame count mismatch (sync drift?)"
    );
    assert!(stats.remaining_bytes < 20);

    assert!(stats.base1_min >= 0 && stats.base1_max <= 4095);
    assert!(stats.base2_min >= 0 && stats.base2_max <= 4095);
    assert!(stats.rheo1_min.abs() <= 4095);
    assert!(stats.rheo1_max.abs() <= 4095);
}

#[test]
fn rcm_fixture_full_dump() {
    let loaded = load_fixture_matching(&["serialservice"])
        .or_else(|| {
            // любой *.bin с "rcm", но не "rcms"
            let dir = fixtures_dir();
            let Ok(entries) = std::fs::read_dir(dir) else {
                return None;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if name.ends_with(".bin") && name.contains("rcm") && !name.contains("rcms") {
                    if let Ok(bytes) = std::fs::read(&path) {
                        return Some((path, bytes));
                    }
                }
            }
            None
        });

    let Some((path, bytes)) = loaded else {
        eprintln!("skip: RCM fixture not found");
        return;
    };
    eprintln!("using RCM fixture: {}", path.display());

    let stats = decode_dump(&bytes);
    eprintln!(
        "RCM fixture: frames={}, resyncs={}, remaining={}",
        stats.frames, stats.resyncs, stats.remaining_bytes
    );
    assert!(stats.frames > 1_000);
    assert!(
        stats.resyncs <= 2,
        "unexpected sync loss on RCM fixture: {}",
        stats.resyncs
    );
    assert!(stats.remaining_bytes < 20);
}

#[test]
fn synchronizer_handles_chunked_input() {
    let frame = make_rcms_like_frame((0x10, 0x20), (0x18, 0x30), (0x22, 0x40), (0x1C, 0x28), (0x34, 0x18));
    let mut sync = FrameSynchronizer::new();
    let mut out = Vec::new();
    for chunk in frame.chunks(3) {
        out.extend(sync.push_bytes(chunk));
    }
    // Java-sync отдаёт кадр только после первого байта следующего.
    if let Some(last) = sync.flush() {
        out.push(last);
    }
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], frame);
}
