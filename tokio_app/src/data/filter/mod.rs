//! Цифровые фильтры (порт `com.ak.digitalfilter` для РКМ / РКМ-С).

mod builder;
mod primitives;

pub use builder::FilterBuilder;
pub use primitives::{DigitalFilter, FilterOut};

/// Применяет фильтр к одному сэмплу (или паре) и возвращает 0..N выходных значений.
pub fn push_one(filter: &mut dyn DigitalFilter, values: &[i32]) -> FilterOut {
    let mut out = FilterOut::new();
    filter.accept(values, &mut out);
    out
}

/// Java `sharpingDecimate`: Hold(size) → Decimate(size) → экстремумы окна
/// (min и max в порядке времени) — сохраняет пики при прореживании для графика.
pub fn sharping_decimate(points: &[(f64, f64)], factor: usize) -> Vec<(f64, f64)> {
    let factor = factor.max(1);
    if factor <= 1 || points.len() <= factor {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity((points.len() / factor + 2) * 2);
    for chunk in points.chunks(factor) {
        let mut min_v = f64::INFINITY;
        let mut max_v = f64::NEG_INFINITY;
        let mut min_t = chunk[0].0;
        let mut max_t = chunk[0].0;
        for &(t, v) in chunk {
            if v < min_v {
                min_v = v;
                min_t = t;
            }
            if v > max_v {
                max_v = v;
                max_t = t;
            }
        }
        if (min_v - max_v).abs() < 1e-12 {
            out.push((min_t, min_v));
        } else if min_t <= max_t {
            out.push((min_t, min_v));
            out.push((max_t, max_v));
        } else {
            out.push((max_t, max_v));
            out.push((min_t, min_v));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharping_keeps_peaks() {
        let mut pts = Vec::new();
        for i in 0..100 {
            let t = i as f64 * 0.005;
            let v = if i == 50 { 100.0 } else { 0.0 };
            pts.push((t, v));
        }
        let d = sharping_decimate(&pts, 10);
        assert!(d.iter().any(|(_, v)| *v > 50.0));
        assert!(d.len() < pts.len());
    }
}
