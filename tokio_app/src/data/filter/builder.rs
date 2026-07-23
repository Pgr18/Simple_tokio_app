//! FilterBuilder — цепочки как в Java `com.ak.digitalfilter.FilterBuilder`.

use super::primitives::*;

pub struct FilterBuilder {
    stages: Vec<Box<dyn DigitalFilter>>,
}

impl FilterBuilder {
    pub fn of() -> Self {
        Self { stages: Vec::new() }
    }

    pub fn build(self) -> Box<dyn DigitalFilter> {
        if self.stages.is_empty() {
            Box::new(Identity)
        } else if self.stages.len() == 1 {
            self.stages.into_iter().next().unwrap()
        } else {
            Box::new(Chain::new(self.stages))
        }
    }

    pub fn operator<F>(mut self, op: F) -> Self
    where
        F: FnMut(i32) -> i32 + Send + 'static,
    {
        self.stages.push(Box::new(OperatorFilter { op }));
        self
    }

    pub fn bi_operator<F>(mut self, op: F) -> Self
    where
        F: FnMut(i32, i32) -> i32 + Send + 'static,
    {
        self.stages.push(Box::new(BiOperatorFilter { op }));
        self
    }

    pub fn fir(mut self, coeffs: &[f64]) -> Self {
        self.stages.push(Box::new(FirFilter::new(coeffs)));
        self
    }

    pub fn decimate(mut self, coeffs: &[f64], factor: usize) -> Self {
        self.stages.push(Box::new(FirFilter::new(coeffs)));
        self.stages.push(Box::new(DecimationFilter::new(factor)));
        self
    }

    pub fn interpolate(mut self, factor: usize, coeffs: &[f64]) -> Self {
        self.stages.push(Box::new(InterpolationFilter::new(factor)));
        self.stages.push(Box::new(FirFilter::new(coeffs)));
        self
    }

    pub fn rrs(mut self) -> Self {
        self.stages.push(Box::new(RrsFilter::default()));
        self
    }

    pub fn peak_to_peak(mut self, size: usize) -> Self {
        self.stages.push(Box::new(PeakToPeakFilter::new(size)));
        self
    }

    pub fn smoothing_impulsive(mut self, size: usize) -> Self {
        self.stages.push(Box::new(SmoothingImpulsive::new(size)));
        self
    }

    /// Добавить уже готовый фильтр в цепочку.
    pub fn chain(mut self, filter: Box<dyn DigitalFilter>) -> Self {
        self.stages.push(filter);
        self
    }
}
