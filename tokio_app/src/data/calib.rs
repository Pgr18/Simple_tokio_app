//! Калибровочные таблицы и FIR-коэффициенты РКМ.

use serde_json::Value;

const RCM_JSON: &str = include_str!("../../resources/rcm/rcm.json");
const BR_F200: &str = include_str!("../../resources/rcm/BR_F200.txt");
const BR_F025: &str = include_str!("../../resources/rcm/BR_F025.txt");
const BR_F005: &str = include_str!("../../resources/rcm/BR_F005.txt");

/// Пары (x, y) для 1D-интерполяции.
#[derive(Debug, Clone)]
pub struct Curve1d {
    xs: Vec<f64>,
    ys: Vec<f64>,
}

impl Curve1d {
    pub fn from_pairs(mut pairs: Vec<(f64, f64)>) -> Self {
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let xs: Vec<_> = pairs.iter().map(|p| p.0).collect();
        let ys: Vec<_> = pairs.iter().map(|p| p.1).collect();
        Self { xs, ys }
    }

    pub fn interp(&self, x: f64) -> f64 {
        if self.xs.is_empty() {
            return 0.0;
        }
        let x = x.clamp(self.xs[0], *self.xs.last().unwrap());
        if self.xs.len() == 1 {
            return self.ys[0];
        }
        // линейная интерполяция (для ≥5 точек Java использует Akima — достаточно близко)
        for i in 0..self.xs.len() - 1 {
            if x >= self.xs[i] && x <= self.xs[i + 1] {
                let t = (x - self.xs[i]) / (self.xs[i + 1] - self.xs[i]);
                return self.ys[i] + t * (self.ys[i + 1] - self.ys[i]);
            }
        }
        *self.ys.last().unwrap()
    }

    pub fn interp_i32(&self, x: i32) -> i32 {
        self.interp(x as f64).round() as i32
    }
}

/// Поверхность (x, y) → z для BASE (CC_ADC, Base_ADC) → mΩ.
#[derive(Debug, Clone)]
pub struct Surface2d {
    /// Срезы: z_ohm → Curve1d(CC → BaseADC), отсортированы по BaseADC при фиксированном CC.
    slices: Vec<(f64, Curve1d)>, // (ohm, cc→base_adc)
}

impl Surface2d {
    pub fn from_ohm_curves(mut slices: Vec<(f64, Curve1d)>) -> Self {
        slices.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        Self { slices }
    }

    /// Java: biOperator(CC, BaseADC) → Ohm (milli).
    pub fn interp(&self, cc: i32, base_adc: i32) -> i32 {
        if self.slices.is_empty() {
            return 0;
        }
        let cc = cc as f64;
        let y = base_adc as f64;

        // Для каждого ohm-среза получаем BaseADC(CC), затем интерполируем ohm по BaseADC.
        let mut points: Vec<(f64, f64)> = self
            .slices
            .iter()
            .map(|(ohm, curve)| (curve.interp(cc), *ohm))
            .collect();
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // уникализируем по x
        points.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
        Curve1d::from_pairs(points).interp(y).round() as i32
    }
}

#[derive(Debug, Clone)]
pub struct RcmCalibration {
    pub br_f200: Vec<f64>,
    pub br_f025: Vec<f64>,
    pub br_f005: Vec<f64>,
    /// Channel 1/2: CC_ADC → Ohm
    pub cc_adc_to_ohm: [Curve1d; 2],
    /// Channel 1/2: CC_ADC → RheoADC@0.26Ω
    pub rheo_adc_to_260: [Curve1d; 2],
    /// Channel 1/2: surface (CC, BaseADC) → mΩ
    pub base_surface: [Surface2d; 2],
}

impl RcmCalibration {
    pub fn load_embedded() -> Self {
        let json: Value = serde_json::from_str(RCM_JSON).expect("rcm.json");
        Self {
            br_f200: parse_coeff_txt(BR_F200),
            br_f025: parse_coeff_txt(BR_F025),
            br_f005: parse_coeff_txt(BR_F005),
            cc_adc_to_ohm: [
                load_cc_adc_to_ohm(&json, 0),
                load_cc_adc_to_ohm(&json, 1),
            ],
            rheo_adc_to_260: [
                load_rheo_260(&json, 1),
                load_rheo_260(&json, 2),
            ],
            base_surface: [
                load_base_surface(&json, 1),
                load_base_surface(&json, 2),
            ],
        }
    }
}

fn parse_coeff_txt(text: &str) -> Vec<f64> {
    text.split_whitespace()
        .filter_map(|s| s.parse::<f64>().ok())
        .collect()
}

fn load_cc_adc_to_ohm(json: &Value, channel_idx: usize) -> Curve1d {
    let obj = &json["Current-carrying electrodes, Ohm : ADC[Channel-1, Channel-2]"];
    let mut pairs = Vec::new();
    if let Some(map) = obj.as_object() {
        for (ohm_str, arr) in map {
            let ohm: f64 = ohm_str.parse().unwrap_or(0.0);
            if let Some(a) = arr.as_array() {
                if let Some(adc) = a.get(channel_idx).and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))) {
                    pairs.push((adc, ohm));
                }
            }
        }
    }
    Curve1d::from_pairs(pairs)
}

fn load_rheo_260(json: &Value, channel: u8) -> Curve1d {
    let key = format!("Channel-{channel}");
    let obj = &json["Rheo 0.26 Ohm : ADC{CurrentCarrying, Rheo}"][&key];
    let mut pairs = Vec::new();
    if let Some(map) = obj.as_object() {
        for (cc, rheo) in map {
            let cc: f64 = cc.parse().unwrap_or(0.0);
            let rheo = rheo.as_f64().or_else(|| rheo.as_i64().map(|i| i as f64)).unwrap_or(0.0);
            pairs.push((cc, rheo));
        }
    }
    Curve1d::from_pairs(pairs)
}

fn load_base_surface(json: &Value, channel: u8) -> Surface2d {
    let key = format!("Channel-{channel}");
    let root = &json["Potential-unit electrodes, Ohm : ADC{CurrentCarrying, Base}"];
    let mut slices = Vec::new();
    if let Some(ohms) = root.as_object() {
        for (ohm_str, chans) in ohms {
            let ohm: f64 = ohm_str.parse().unwrap_or(0.0);
            let chan = &chans[&key];
            let mut pairs = Vec::new();
            if let Some(map) = chan.as_object() {
                for (cc, base) in map {
                    let cc: f64 = cc.parse().unwrap_or(0.0);
                    let base = base.as_f64().or_else(|| base.as_i64().map(|i| i as f64)).unwrap_or(0.0);
                    pairs.push((cc, base));
                }
            }
            slices.push((ohm, Curve1d::from_pairs(pairs)));
        }
    }
    Surface2d::from_ohm_curves(slices)
}

/// Уровни поверхности BASE из имён CCU_VADC_* (для справки / тестов).
#[allow(dead_code)]
pub fn milli_suffix_to_ohm(suffix: u32) -> f64 {
    suffix as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_fir_lengths() {
        let c = RcmCalibration::load_embedded();
        assert_eq!(c.br_f200.len(), 22);
        assert_eq!(c.br_f025.len(), 25);
        assert_eq!(c.br_f005.len(), 10);
    }

    #[test]
    fn cc_curve_monotonic() {
        let c = RcmCalibration::load_embedded();
        let low = c.cc_adc_to_ohm[0].interp_i32(188);
        let high = c.cc_adc_to_ohm[0].interp_i32(1192);
        assert!(high > low);
    }
}
