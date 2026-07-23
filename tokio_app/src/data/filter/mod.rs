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
