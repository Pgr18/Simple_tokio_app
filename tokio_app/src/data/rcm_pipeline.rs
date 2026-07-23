//! Пайплайн переменных РКМ / РКМ-С / calibration.
//!
//! Порядок каналов — как Java `RcmInVariable` / `RcmConverter`:
//!   0 RHEO_1, 1 BASE_1, 2 RHEO_2, 3 ECG, 4 BASE_2,
//!   5 RHEO_1X, 6 QS_1, 7 RHEO_2X, 8 ECG_X, 9 QS_2
//!
//! RCMS = полный `RcmOut` + `QS_2 ← QS_1` (как `RcmsOutVariable`).
//!
//! Фильтры по каналам включаются независимо (`ChannelFilterFlags`).

use crate::com_port::decode::{to_int, to_signed_int};
use crate::data::calib::RcmCalibration;
use crate::data::filter::{push_one, DigitalFilter, FilterBuilder, FilterOut};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const SAMPLE_RATE_HZ: f64 = 200.0;

/// Какие каналы показывать через DSP (остальные — сырой ADC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelFilterFlags {
    pub rheo1: bool,
    pub base1: bool,
    pub ecg: bool,
    pub rheo2: bool,
    pub base2: bool,
}

impl Default for ChannelFilterFlags {
    fn default() -> Self {
        // Типичный старт: ЭКГ+BASE, РЕО сырой.
        Self {
            rheo1: false,
            base1: true,
            ecg: true,
            rheo2: false,
            base2: true,
        }
    }
}

impl ChannelFilterFlags {
    pub fn any(&self) -> bool {
        self.rheo1 || self.base1 || self.ecg || self.rheo2 || self.base2
    }

    pub fn all_off() -> Self {
        Self {
            rheo1: false,
            base1: false,
            ecg: false,
            rheo2: false,
            base2: false,
        }
    }
}

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
    flags: ChannelFilterFlags,
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
    /// Выравнивание DSP: RHEO1, BASE1, QS1, ECG, RHEO2, BASE2, QS2.
    align: [VecDeque<i32>; 7],
    /// Сырой ADC 1:1 с входом (для каналов с выключенным фильтром).
    raw_rheo1: VecDeque<i32>,
    raw_base1: VecDeque<i32>,
    raw_ecg: VecDeque<i32>,
    raw_rheo2: VecDeque<i32>,
    raw_base2: VecDeque<i32>,
}

impl RcmPipeline {
    pub fn new(profile: RcmProfile) -> Self {
        Self::new_with_flags(profile, ChannelFilterFlags::default())
    }

    pub fn new_bin_view(profile: RcmProfile) -> Self {
        Self::new(profile)
    }

    pub fn new_with_flags(profile: RcmProfile, flags: ChannelFilterFlags) -> Self {
        let calib = RcmCalibration::load_embedded();
        Self::from_calib(profile, calib, flags)
    }

    pub fn from_calib(
        profile: RcmProfile,
        calib: RcmCalibration,
        flags: ChannelFilterFlags,
    ) -> Self {
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
            flags,
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
            raw_rheo1: VecDeque::new(),
            raw_base1: VecDeque::new(),
            raw_ecg: VecDeque::new(),
            raw_rheo2: VecDeque::new(),
            raw_base2: VecDeque::new(),
        }
    }

    pub fn profile(&self) -> RcmProfile {
        self.profile
    }

    pub fn flags(&self) -> ChannelFilterFlags {
        self.flags
    }

    pub fn set_flags(&mut self, flags: ChannelFilterFlags) {
        if flags != self.flags {
            let profile = self.profile;
            let calib = self.calib.clone();
            *self = Self::from_calib(profile, calib, flags);
        }
    }

    pub fn set_profile(&mut self, profile: RcmProfile) {
        if self.profile != profile {
            let flags = self.flags;
            let calib = self.calib.clone();
            *self = Self::from_calib(profile, calib, flags);
        }
    }

    pub fn reset(&mut self) {
        let profile = self.profile;
        let calib = self.calib.clone();
        let flags = self.flags;
        *self = Self::from_calib(profile, calib, flags);
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
        // Сырой ADC всегда копим (для каналов без фильтра).
        self.raw_rheo1
            .push_back(inn.values[RcmInChannel::Rheo1 as usize]);
        self.raw_base1
            .push_back(inn.values[RcmInChannel::Base1 as usize]);
        self.raw_ecg
            .push_back(inn.values[RcmInChannel::Ecg as usize]);
        self.raw_rheo2
            .push_back(inn.values[RcmInChannel::Rheo2 as usize]);
        self.raw_base2
            .push_back(inn.values[RcmInChannel::Base2 as usize]);

        if !self.flags.any() {
            // Все сырые — 1:1 без прогрева FIR.
            return vec![self.pop_raw_only()];
        }

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

        let mut out = Vec::new();
        while self.align.iter().all(|q| !q.is_empty()) && self.raw_ready() {
            let filt = RcmOutSample {
                rheo1: self.align[0].pop_front().unwrap(),
                base1: self.align[1].pop_front().unwrap(),
                qs1: self.align[2].pop_front().unwrap(),
                ecg: self.align[3].pop_front().unwrap(),
                rheo2: self.align[4].pop_front().unwrap(),
                base2: self.align[5].pop_front().unwrap(),
                qs2: self.align[6].pop_front().unwrap(),
            };
            let raw = self.pop_raw_only();
            out.push(self.mix(filt, raw));
        }

        for q in &mut self.align {
            while q.len() > 2000 {
                q.pop_front();
            }
        }
        self.trim_raw(4000);
        out
    }

    fn raw_ready(&self) -> bool {
        !self.raw_rheo1.is_empty()
            && !self.raw_base1.is_empty()
            && !self.raw_ecg.is_empty()
            && !self.raw_rheo2.is_empty()
            && !self.raw_base2.is_empty()
    }

    fn pop_raw_only(&mut self) -> RcmOutSample {
        RcmOutSample {
            rheo1: self.raw_rheo1.pop_front().unwrap_or(0),
            base1: self.raw_base1.pop_front().unwrap_or(0),
            qs1: 0,
            ecg: self.raw_ecg.pop_front().unwrap_or(0),
            rheo2: self.raw_rheo2.pop_front().unwrap_or(0),
            base2: self.raw_base2.pop_front().unwrap_or(0),
            qs2: 0,
        }
    }

    fn mix(&self, filt: RcmOutSample, raw: RcmOutSample) -> RcmOutSample {
        let f = self.flags;
        RcmOutSample {
            rheo1: if f.rheo1 { filt.rheo1 } else { raw.rheo1 },
            base1: if f.base1 { filt.base1 } else { raw.base1 },
            qs1: filt.qs1,
            ecg: if f.ecg { filt.ecg } else { raw.ecg },
            rheo2: if f.rheo2 { filt.rheo2 } else { raw.rheo2 },
            base2: if f.base2 { filt.base2 } else { raw.base2 },
            qs2: filt.qs2,
        }
    }

    fn trim_raw(&mut self, max: usize) {
        while self.raw_rheo1.len() > max {
            self.raw_rheo1.pop_front();
            self.raw_base1.pop_front();
            self.raw_ecg.pop_front();
            self.raw_rheo2.pop_front();
            self.raw_base2.pop_front();
        }
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
        raw[4] = 0x2A & !1;
        raw[5] = 0x0C;
        let inn = decode_rcm_in(&raw);
        assert_eq!(
            inn.values[RcmInChannel::Rheo2 as usize],
            to_signed_int(raw[4], raw[5])
        );
    }

    #[test]
    fn java_rcm_converter_frame_decodes() {
        let mut raw = [0u8; 20];
        for i in 0..10 {
            raw[i * 2] = 0x02;
            raw[i * 2 + 1] = 0x04;
        }
        raw[18] = 0x01;
        raw[19] = 0x00;
        let inn = decode_rcm_in(&raw);
        assert_ne!(inn.values[RcmInChannel::Rheo1 as usize], 0);
        assert_ne!(inn.values[RcmInChannel::Base1 as usize], 0);
        assert_ne!(inn.values[RcmInChannel::Rheo2 as usize], 0);
        assert_ne!(inn.values[RcmInChannel::Ecg as usize], 0);
    }

    #[test]
    fn measure_io_ratio() {
        let mut p = RcmPipeline::new(RcmProfile::Rcms);
        let mut raw = [0u8; 20];
        raw[18] = 0x01;
        raw[19] = 0x00;
        let mut total_out = 0usize;
        let n = 2000usize;
        for i in 0..n {
            raw[0] = ((i as u8) << 1) & 0x7E;
            total_out += p.process_raw(&raw).len();
        }
        let ratio = total_out as f64 / n as f64;
        eprintln!("in={n} out={total_out} ratio={ratio:.3}");
        assert!(total_out > 0);
    }

    #[test]
    fn all_off_emits_immediately() {
        let mut p = RcmPipeline::new_with_flags(RcmProfile::Rcms, ChannelFilterFlags::all_off());
        let mut raw = [0u8; 20];
        raw[18] = 0x01;
        raw[19] = 0x00;
        let o = p.process_raw(&raw);
        assert_eq!(o.len(), 1);
    }
}
