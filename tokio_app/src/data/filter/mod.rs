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

/// Java `sharpingDecimate`: Hold(size) → Decimate(size) → одна точка на окно:
/// min или max — что дальше от предыдущего значения (сохраняет пики без «пил»).
pub fn sharping_decimate(points: &[(f64, f64)], factor: usize) -> Vec<(f64, f64)> {
    let factor = factor.max(1);
    if factor <= 1 || points.len() <= factor {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() / factor + 2);
    let mut prev = points[0].1;
    for chunk in points.chunks(factor) {
        let mut min_v = f64::INFINITY;
        let mut max_v = f64::NEG_INFINITY;
        let mut t_end = chunk[0].0;
        for &(t, v) in chunk {
            if v < min_v {
                min_v = v;
            }
            if v > max_v {
                max_v = v;
            }
            t_end = t;
        }
        let next = if (prev - max_v).abs() > (prev - min_v).abs() {
            max_v
        } else {
            min_v
        };
        prev = next;
        out.push((t_end, next));
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
