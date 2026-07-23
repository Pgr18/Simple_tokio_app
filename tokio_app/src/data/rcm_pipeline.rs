//! Пайплайн переменных РКМ / РКМ-С / calibration.
//!
//! Порядок каналов — как Java `RcmInVariable` / `RcmConverter`:
//!   0 RHEO_1, 1 BASE_1, 2 RHEO_2, 3 ECG, 4 BASE_2,
//!   5 RHEO_1X, 6 QS_1, 7 RHEO_2X, 8 ECG_X, 9 QS_2
//!
//! RCMS = полный `RcmOut` + `QS_2 ← QS_1` (как `RcmsOutVariable`).

use crate::com_port::decode::{to_int, to_signed_int};
use crate::data::calib::RcmCalibration;
use crate::data::filter::{push_one, DigitalFilter, FilterBuilder, FilterOut};
use std::collections::VecDeque;

pub const SAMPLE_RATE_HZ: f64 = 200.0;

/// Индексы пар в 20-байтном кадре (= ordinal Java `RcmInVariable`).
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RcmInChannel {
    Rheo1 = 0,
    Base1 = 1,
    Rheo2 = 2,
    Ecg = 3,
    Base2 = 4,
    Rheo1x = 5,
    Qs1 = 6,
    Rheo2x = 7,
    EcgX = 8,
    Qs2 = 9,
}

impl RcmInChannel {
    pub const COUNT: usize = 10;

    fn is_signed(self) -> bool {
        matches!(self, Self::Rheo1 | Self::Rheo2 | Self::Ecg)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RcmInSample {
    pub values: [i32; RcmInChannel::COUNT],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RcmOutSample {
    pub rheo1: i32,
    pub base1: i32,
    pub qs1: i32,
    pub ecg: i32,
    pub rheo2: i32,
    pub base2: i32,
    pub qs2: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RcmProfile {
    Rcm,
    Rcms,
    Calibration,
}

impl Default for RcmProfile {
    fn default() -> Self {
        Self::Rcms
    }
}

/// Декод как Java `RcmConverter` + `RcmInVariable.toSignedFilter` (без INVERSE).
pub fn decode_rcm_in(raw: &[u8; 20]) -> RcmInSample {
    let mut values = [0i32; RcmInChannel::COUNT];
    for i in 0..RcmInChannel::COUNT {
        let off = i * 2;
        let ch = match i {
            0 => RcmInChannel::Rheo1,
            1 => RcmInChannel::Base1,
            2 => RcmInChannel::Rheo2,
            3 => RcmInChannel::Ecg,
            4 => RcmInChannel::Base2,
            5 => RcmInChannel::Rheo1x,
            6 => RcmInChannel::Qs1,
            7 => RcmInChannel::Rheo2x,
            8 => RcmInChannel::EcgX,
            _ => RcmInChannel::Qs2,
        };
        values[i] = if ch.is_signed() {
            to_signed_int(raw[off], raw[off + 1])
        } else {
            to_int(raw[off], raw[off + 1])
        };
    }
    RcmInSample { values }
}

struct ChannelFilter {
    filter: Box<dyn DigitalFilter>,
    inputs: Vec<usize>,
}

pub struct RcmPipeline {
    profile: RcmProfile,
    calib: RcmCalibration,
    rheo1: ChannelFilter,
    base1: ChannelFilter,
    qs1: ChannelFilter,
    ecg: ChannelFilter,
    rheo2: ChannelFilter,
    base2: ChannelFilter,
    qs2: ChannelFilter,
    cc_adc: ChannelFilter,
    base_adc: ChannelFilter,
    rheo_adc: ChannelFilter,
    avg_rheo: ChannelFilter,
    min_rheo: ChannelFilter,
    /// Выравнивание каналов по задержке FIR (как Java LinkedConverter).
    align: [VecDeque<i32>; 7],
}

impl RcmPipeline {
    pub fn new(profile: RcmProfile) -> Self {
        let calib = RcmCalibration::load_embedded();
        Self::from_calib(profile, calib)
    }

    pub fn from_calib(profile: RcmProfile, calib: RcmCalibration) -> Self {
        // RCM/RCMS: полный RcmOut (Ohm / FIR).
        // На RCMS физический QS_2 часто пустой (последний байт кадра = 0) —
        // для канала 2 берём QS_1 (как смысл RcmsOutVariable.QS_2 ← QS_1).
        let qs_ch2 = match profile {
            RcmProfile::Rcms => RcmInChannel::Qs1,
            _ => RcmInChannel::Qs2,
        };

        let rheo1 = make_rheo_filter(&calib, 0, RcmInChannel::Rheo1, RcmInChannel::Qs1);
        let base1 = make_base_filter(&calib, 0, RcmInChannel::Base1, RcmInChannel::Qs1);
        let qs1 = make_qs_filter(&calib, 0, RcmInChannel::Qs1);
        let ecg = ChannelFilter {
            filter: FilterBuilder::of().smoothing_impulsive(4).build(),
            inputs: vec![RcmInChannel::Ecg as usize],
        };
        let rheo2 = make_rheo_filter(&calib, 1, RcmInChannel::Rheo2, qs_ch2);
        let base2 = make_base_filter(&calib, 1, RcmInChannel::Base2, qs_ch2);
        let qs2 = make_qs_filter(&calib, 1, qs_ch2);

        Self {
            profile,
            calib,
            rheo1,
            base1,
            qs1,
            ecg,
            rheo2,
            base2,
            qs2,
            cc_adc: ChannelFilter {
                filter: FilterBuilder::of().rrs().build(),
                inputs: vec![RcmInChannel::Qs1 as usize],
            },
            base_adc: ChannelFilter {
                filter: FilterBuilder::of().rrs().build(),
                inputs: vec![RcmInChannel::Base1 as usize],
            },
            rheo_adc: ChannelFilter {
                filter: FilterBuilder::of().build(),
                inputs: vec![RcmInChannel::Rheo1 as usize],
            },
            avg_rheo: ChannelFilter {
                filter: FilterBuilder::of().rrs().build(),
                inputs: vec![RcmInChannel::Rheo1 as usize],
            },
            min_rheo: ChannelFilter {
                filter: FilterBuilder::of().peak_to_peak(400).build(),
                inputs: vec![RcmInChannel::Rheo1 as usize],
            },
            align: std::array::from_fn(|_| VecDeque::new()),
        }
    }

    pub fn profile(&self) -> RcmProfile {
        self.profile
    }

    pub fn set_profile(&mut self, profile: RcmProfile) {
        if self.profile != profile {
            *self = Self::from_calib(profile, self.calib.clone());
        }
    }

    /// Полный сброс состояния FIR (при включении фильтров / смене профиля).
    pub fn reset(&mut self) {
        *self = Self::from_calib(self.profile, self.calib.clone());
    }

    pub fn process_raw(&mut self, raw: &[u8; 20]) -> Vec<RcmOutSample> {
        let inn = decode_rcm_in(raw);
        self.process_in(&inn)
    }

    pub fn process_in(&mut self, inn: &RcmInSample) -> Vec<RcmOutSample> {
        match self.profile {
            RcmProfile::Calibration => self.process_calibration(inn),
            RcmProfile::Rcm | RcmProfile::Rcms => self.process_out(inn),
        }
    }

    fn process_out(&mut self, inn: &RcmInSample) -> Vec<RcmOutSample> {
        let r1 = run_channel(&mut self.rheo1, inn);
        let b1 = run_channel(&mut self.base1, inn);
        let q1 = run_channel(&mut self.qs1, inn);
        let e = run_channel(&mut self.ecg, inn);
        let r2 = run_channel(&mut self.rheo2, inn);
        let b2 = run_channel(&mut self.base2, inn);
        let q2 = if self.profile == RcmProfile::Rcms {
            q1.clone()
        } else {
            run_channel(&mut self.qs2, inn)
        };

        enqueue(&mut self.align[0], &r1);
        enqueue(&mut self.align[1], &b1);
        enqueue(&mut self.align[2], &q1);
        enqueue(&mut self.align[3], &e);
        enqueue(&mut self.align[4], &r2);
        enqueue(&mut self.align[5], &b2);
        enqueue(&mut self.align[6], &q2);

        // Не выдаём кадр, пока все каналы не догнали задержку BASE (~1.7 с).
        let mut out = Vec::new();
        while self.align.iter().all(|q| !q.is_empty()) {
            out.push(RcmOutSample {
                rheo1: self.align[0].pop_front().unwrap(),
                base1: self.align[1].pop_front().unwrap(),
                qs1: self.align[2].pop_front().unwrap(),
                ecg: self.align[3].pop_front().unwrap(),
                rheo2: self.align[4].pop_front().unwrap(),
                base2: self.align[5].pop_front().unwrap(),
                qs2: self.align[6].pop_front().unwrap(),
            });
        }
        // Защита от раздувания, если один канал «молчит».
        for q in &mut self.align {
            while q.len() > 2000 {
                q.pop_front();
            }
        }
        out
    }

    fn process_calibration(&mut self, inn: &RcmInSample) -> Vec<RcmOutSample> {
        let cc = run_channel(&mut self.cc_adc, inn);
        let base = run_channel(&mut self.base_adc, inn);
        let rheo = run_channel(&mut self.rheo_adc, inn);
        let avg = run_channel(&mut self.avg_rheo, inn);
        let min = run_channel(&mut self.min_rheo, inn);
        zip_outputs([&rheo, &base, &cc, &avg, &min, &FilterOut::new(), &FilterOut::new()])
    }
}

fn make_rheo_filter(
    calib: &RcmCalibration,
    ch: usize,
    rheo: RcmInChannel,
    qs: RcmInChannel,
) -> ChannelFilter {
    let rheo260 = calib.rheo_adc_to_260[ch].clone();
    ChannelFilter {
        filter: FilterBuilder::of()
            .bi_operator(move |cc_adc, rheo_adc| {
                let denom = rheo260.interp_i32(cc_adc);
                if denom == 0 {
                    0
                } else {
                    ((260.0 * 1000.0 * rheo_adc as f64) / denom as f64).round() as i32
                }
            })
            .smoothing_impulsive(4)
            .build(),
        // Java: QS, RHEO
        inputs: vec![qs as usize, rheo as usize],
    }
}

fn make_base_filter(
    calib: &RcmCalibration,
    ch: usize,
    base: RcmInChannel,
    qs: RcmInChannel,
) -> ChannelFilter {
    let surface = calib.base_surface[ch].clone();
    let br200 = calib.br_f200.clone();
    let br025 = calib.br_f025.clone();
    let br005 = calib.br_f005.clone();
    // Java RcmOutVariable.getBaseFilter: surface + multi-rate FIR bank + impulsive(4).
    // Delay ≈ 377 samples (~1.885 s). DSP must run off the UI thread.
    ChannelFilter {
        filter: FilterBuilder::of()
            .bi_operator(move |cc, base_adc| surface.interp(cc, base_adc))
            .decimate(&br200, 8)
            .decimate(&br025, 5)
            .fir(&br005)
            .interpolate(5, &br025)
            .interpolate(8, &br200)
            .smoothing_impulsive(4)
            .build(),
        inputs: vec![qs as usize, base as usize],
    }
}

fn make_qs_filter(calib: &RcmCalibration, ch: usize, qs: RcmInChannel) -> ChannelFilter {
    let curve = calib.cc_adc_to_ohm[ch].clone();
    ChannelFilter {
        filter: FilterBuilder::of()
            .operator(move |adc| curve.interp_i32(adc))
            .smoothing_impulsive(4)
            .build(),
        inputs: vec![qs as usize],
    }
}

fn run_channel(ch: &mut ChannelFilter, inn: &RcmInSample) -> FilterOut {
    let inputs: Vec<i32> = ch.inputs.iter().map(|&i| inn.values[i]).collect();
    push_one(ch.filter.as_mut(), &inputs)
}

fn enqueue(dst: &mut VecDeque<i32>, src: &FilterOut) {
    dst.extend(src.values.iter().copied());
}

fn zip_outputs(channels: [&FilterOut; 7]) -> Vec<RcmOutSample> {
    let n = channels.iter().map(|c| c.values.len()).max().unwrap_or(0);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(RcmOutSample {
            rheo1: channels[0].values.get(i).copied().unwrap_or(0),
            base1: channels[1].values.get(i).copied().unwrap_or(0),
            qs1: channels[2].values.get(i).copied().unwrap_or(0),
            ecg: channels[3].values.get(i).copied().unwrap_or(0),
            rheo2: channels[4].values.get(i).copied().unwrap_or(0),
            base2: channels[5].values.get(i).copied().unwrap_or(0),
            qs2: channels[6].values.get(i).copied().unwrap_or(0),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_rheo2_is_pair_2() {
        let mut raw = [0u8; 20];
        for i in 0..10 {
            raw[i * 2] = if i == 0 { 0x02 } else { 0x03 };
            raw[i * 2 + 1] = 0x04;
        }
        // РЕО-2 = пара 2 (offsets 4–5)
        raw[4] = 0x2A & !1;
        raw[5] = 0x4C & !1;
        let inn = decode_rcm_in(&raw);
        assert_eq!(
            inn.values[RcmInChannel::Rheo2 as usize],
            to_signed_int(raw[4], raw[5])
        );
    }

    #[test]
    fn measure_io_ratio() {
        let mut p = RcmPipeline::new(RcmProfile::Rcms);
        let mut raw = [0u8; 20];
        for i in 0..10 {
            raw[i * 2] = if i == 0 { 0x02 } else { 0x12 };
            raw[i * 2 + 1] = 0x04;
        }
        let n = 2000usize;
        let mut total_out = 0usize;
        for _ in 0..n {
            total_out += p.process_raw(&raw).len();
        }
        let ratio = total_out as f64 / n as f64;
        eprintln!("in={n} out={total_out} ratio={ratio:.3}");
        // Ось времени = out/200; если ratio << 1, запись «ползёт».
        assert!(
            ratio > 0.85,
            "output too sparse vs input: ratio={ratio:.3} ({total_out}/{n})"
        );
    }

    #[test]
    fn java_rcm_converter_frame_decodes() {
        // Кадр из RcmConverterTest / RcmBytesInterceptorTest
        let raw: [u8; 20] = [
            0xf6, 0xdc, 0x83, 0xb8, 0xfb, 0xc4, 0x83, 0x84, 0x91, 0xa2, 0xf9, 0x9e, 0x81, 0x80,
            0xfb, 0xb2, 0x81, 0xf6, 0x81, 0x80,
        ];
        let inn = decode_rcm_in(&raw);
        assert_ne!(inn.values[RcmInChannel::Rheo1 as usize], 0);
        assert_ne!(inn.values[RcmInChannel::Base1 as usize], 0);
        assert_ne!(inn.values[RcmInChannel::Rheo2 as usize], 0);
        assert_ne!(inn.values[RcmInChannel::Ecg as usize], 0);
        // QS_1 в этом кадре = 0 (0x81,0x80 → маска 0x7E даёт 0) — как в Java-фикстуре.

        let mut p = RcmPipeline::new(RcmProfile::Rcm);
        let mut got = None;
        for _ in 0..900 {
            for s in p.process_raw(&raw) {
                got = Some(s);
            }
        }
        let s = got.expect("pipeline should produce after delay");
        assert!(s.rheo1.abs() > 100, "rheo1={}", s.rheo1);
        assert!(s.base1 != 0, "base1={}", s.base1);
        assert!(s.ecg.abs() > 0, "ecg={}", s.ecg);
    }
}
