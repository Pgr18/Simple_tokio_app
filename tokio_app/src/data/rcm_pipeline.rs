//! Пайплайн переменных РКМ / РКМ-С / calibration.
//!
//! Физическая раскладка кадра (по дампам РКМ/РКМ-С), не путать с порядком enum в Java:
//!   0: РЕО-1, 1: BASE-1, 2: unused, 3: ЭКГ, 4: BASE-2,
//!   5: unused, 6: unused, 7: РЕО-2, 8: unused, 9: service/QS_2

use crate::com_port::decode::{convert, to_int};
use crate::data::calib::RcmCalibration;
use crate::data::filter::{push_one, DigitalFilter, FilterBuilder, FilterOut};

pub const SAMPLE_RATE_HZ: f64 = 200.0;

/// Индексы *пар* в 20-байтном кадре (физические).
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysChannel {
    Rheo1 = 0,
    Base1 = 1,
    Unused2 = 2,
    Ecg = 3,
    Base2 = 4,
    Unused5 = 5,
    Unused6 = 6,
    Rheo2 = 7,
    Unused8 = 8,
    Service = 9,
}

impl PhysChannel {
    pub const COUNT: usize = 10;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RcmInSample {
    /// 10 значений: для bipolar уже через `convert(..., true)`, для uni — `to_int`.
    pub values: [i32; PhysChannel::COUNT],
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

/// Декод по физической раскладке (как в fixture-дампах).
pub fn decode_rcm_in(raw: &[u8; 20]) -> RcmInSample {
    let mut values = [0i32; PhysChannel::COUNT];
    for i in 0..PhysChannel::COUNT {
        let off = i * 2;
        let bipolar = matches!(
            i,
            x if x == PhysChannel::Rheo1 as usize
                || x == PhysChannel::Rheo2 as usize
                || x == PhysChannel::Ecg as usize
        );
        values[i] = convert(raw[off], raw[off + 1], bipolar);
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
}

impl RcmPipeline {
    pub fn new(profile: RcmProfile) -> Self {
        let calib = RcmCalibration::load_embedded();
        Self::from_calib(profile, calib)
    }

    pub fn from_calib(profile: RcmProfile, calib: RcmCalibration) -> Self {
        // Для живого отображения RCMS: лёгкое impulsive-smoothing на уже
        // декодированных физических каналах (без QS/калибровки — слоты QS не используются).
        // Полная Ohm/FIR-калибровка — профиль Rcm (когда QS валиден).
        let light = |ch: PhysChannel| ChannelFilter {
            filter: FilterBuilder::of().smoothing_impulsive(4).build(),
            inputs: vec![ch as usize],
        };

        let (rheo1, base1, qs1, ecg, rheo2, base2, qs2) = match profile {
            RcmProfile::Rcms | RcmProfile::Calibration => (
                light(PhysChannel::Rheo1),
                light(PhysChannel::Base1),
                light(PhysChannel::Unused6), // нет QS — показываем сырой слот как есть
                light(PhysChannel::Ecg),
                light(PhysChannel::Rheo2),
                light(PhysChannel::Base2),
                light(PhysChannel::Service),
            ),
            RcmProfile::Rcm => (
                make_rheo_filter(&calib, PhysChannel::Rheo1, PhysChannel::Unused6),
                make_base_filter(&calib, 0, PhysChannel::Base1, PhysChannel::Unused6),
                make_qs_filter(&calib, 0, PhysChannel::Unused6),
                light(PhysChannel::Ecg),
                make_rheo_filter(&calib, PhysChannel::Rheo2, PhysChannel::Service),
                make_base_filter(&calib, 1, PhysChannel::Base2, PhysChannel::Service),
                make_qs_filter(&calib, 1, PhysChannel::Service),
            ),
        };

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
                inputs: vec![PhysChannel::Unused6 as usize],
            },
            base_adc: ChannelFilter {
                filter: FilterBuilder::of().rrs().build(),
                inputs: vec![PhysChannel::Base1 as usize],
            },
            rheo_adc: ChannelFilter {
                filter: FilterBuilder::of().build(),
                inputs: vec![PhysChannel::Rheo1 as usize],
            },
            avg_rheo: ChannelFilter {
                filter: FilterBuilder::of().rrs().build(),
                inputs: vec![PhysChannel::Rheo1 as usize],
            },
            min_rheo: ChannelFilter {
                filter: FilterBuilder::of().peak_to_peak(400).build(),
                inputs: vec![PhysChannel::Rheo1 as usize],
            },
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
        zip_outputs([&r1, &b1, &q1, &e, &r2, &b2, &q2])
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

fn make_rheo_filter(calib: &RcmCalibration, rheo: PhysChannel, qs: PhysChannel) -> ChannelFilter {
    let ch = if rheo == PhysChannel::Rheo1 { 0 } else { 1 };
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
    base: PhysChannel,
    qs: PhysChannel,
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

fn make_qs_filter(calib: &RcmCalibration, ch: usize, qs: PhysChannel) -> ChannelFilter {
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
    fn physical_rheo2_is_pair_7() {
        let mut raw = [0u8; 20];
        // РЕО-2 at offsets 14-15
        raw[14] = 0x2A & !1;
        raw[15] = 0x4C & !1;
        for i in 0..10 {
            if i == 7 {
                continue;
            }
            raw[i * 2] = if i == 0 { 0x02 } else { 0x03 };
            raw[i * 2 + 1] = 0x04;
        }
        let inn = decode_rcm_in(&raw);
        assert_eq!(
            inn.values[PhysChannel::Rheo2 as usize],
            convert(raw[14], raw[15], true)
        );
        assert_ne!(
            inn.values[PhysChannel::Rheo2 as usize],
            convert(raw[4], raw[5], true)
        );
    }

    #[test]
    fn pipeline_rcms_produces() {
        let mut p = RcmPipeline::new(RcmProfile::Rcms);
        let mut raw = [0u8; 20];
        for i in 0..10 {
            raw[i * 2] = if i == 0 { 0x02 } else { 0x03 };
            raw[i * 2 + 1] = 0x04;
        }
        let mut total = 0;
        for _ in 0..40 {
            total += p.process_raw(&raw).len();
        }
        assert!(total > 0);
    }

    #[allow(dead_code)]
    fn _keep_to_int(h: u8, l: u8) -> i32 {
        to_int(h, l)
    }
}
