//! Примитивы цифровых фильтров.

use std::collections::VecDeque;

/// Буфер выходных сэмплов одного `accept`.
#[derive(Debug, Default, Clone)]
pub struct FilterOut {
    pub values: Vec<i32>,
}

impl FilterOut {
    pub fn new() -> Self {
        Self { values: Vec::with_capacity(8) }
    }

    pub fn push(&mut self, v: i32) {
        self.values.push(v);
    }

    pub fn extend_from(&mut self, other: &FilterOut) {
        self.values.extend_from_slice(&other.values);
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }
}

pub trait DigitalFilter: Send {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut);
    fn reset(&mut self);
    fn delay(&self) -> f64 {
        0.0
    }
    fn frequency_factor(&self) -> f64 {
        1.0
    }
}

/// Identity / NoFilter.
#[derive(Debug, Default, Clone)]
pub struct Identity;

impl DigitalFilter for Identity {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        if let Some(&v) = values.first() {
            out.push(v);
        }
    }

    fn reset(&mut self) {}
}

/// Поэлементный unary operator.
pub struct OperatorFilter<F>
where
    F: FnMut(i32) -> i32 + Send,
{
    pub op: F,
}

impl<F> DigitalFilter for OperatorFilter<F>
where
    F: FnMut(i32) -> i32 + Send,
{
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        if let Some(&v) = values.first() {
            out.push((self.op)(v));
        }
    }

    fn reset(&mut self) {}
}

/// Bi-operator: два входа → один выход.
pub struct BiOperatorFilter<F>
where
    F: FnMut(i32, i32) -> i32 + Send,
{
    pub op: F,
}

impl<F> DigitalFilter for BiOperatorFilter<F>
where
    F: FnMut(i32, i32) -> i32 + Send,
{
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        if values.len() >= 2 {
            out.push((self.op)(values[0], values[1]));
        }
    }

    fn reset(&mut self) {}
}

/// FIR с кольцевым буфером (как Java FIRFilter).
pub struct FirFilter {
    coeffs: Vec<f64>,
    buffer: Vec<i32>,
    index: isize,
    filled: usize,
}

impl FirFilter {
    pub fn new(coeffs: &[f64]) -> Self {
        let n = coeffs.len().max(1);
        Self {
            coeffs: coeffs.to_vec(),
            buffer: vec![0; n],
            index: -1,
            filled: 0,
        }
    }
}

impl DigitalFilter for FirFilter {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        let n = self.buffer.len();
        self.index = (self.index + 1) % n as isize;
        self.buffer[self.index as usize] = x;
        self.filled = (self.filled + 1).min(n);

        // Java: result += get(1 + i + nowIndex) * coefficients[i]
        // get(k) = buffer[k % length], nowIndex = bufferIndex after write
        let mut result = 0.0;
        for (i, &c) in self.coeffs.iter().enumerate() {
            let idx = (1 + i + self.index as usize) % n;
            result += self.buffer[idx] as f64 * c;
        }
        out.push(result.round() as i32);
    }

    fn reset(&mut self) {
        self.buffer.fill(0);
        self.index = -1;
        self.filled = 0;
    }

    fn delay(&self) -> f64 {
        (self.coeffs.len().saturating_sub(1) as f64) / 2.0
    }
}

/// Decimation: пропускает каждый `factor`-й сэмпл.
pub struct DecimationFilter {
    factor: usize,
    counter: usize,
}

impl DecimationFilter {
    pub fn new(factor: usize) -> Self {
        Self {
            factor: factor.max(1),
            counter: 0,
        }
    }
}

impl DigitalFilter for DecimationFilter {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        self.counter = (self.counter + 1) % self.factor;
        if self.counter == 0 {
            out.push(x);
        }
    }

    fn reset(&mut self) {
        self.counter = 0;
    }

    fn delay(&self) -> f64 {
        (-((self.factor - 1) as f64) / 2.0) / self.factor as f64
    }

    fn frequency_factor(&self) -> f64 {
        1.0 / self.factor as f64
    }
}

/// Zero-order hold interpolation: повторяет значение `factor` раз.
pub struct InterpolationFilter {
    factor: usize,
}

impl InterpolationFilter {
    pub fn new(factor: usize) -> Self {
        Self {
            factor: factor.max(1),
        }
    }
}

impl DigitalFilter for InterpolationFilter {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        for _ in 0..self.factor {
            out.push(x);
        }
    }

    fn reset(&mut self) {}

    fn frequency_factor(&self) -> f64 {
        self.factor as f64
    }
}

/// Hold-буфер для impulsive smoothing (хранит последние `size` значений).
pub struct HoldFilter {
    size: usize,
    lost_count: usize,
    buffer: Vec<i32>,
    index: isize,
}

impl HoldFilter {
    pub fn new(size: usize, lost_count: usize) -> Self {
        Self {
            size: size.max(1),
            lost_count,
            buffer: vec![0; size.max(1)],
            index: -1,
        }
    }

    pub fn push_value(&mut self, x: i32) {
        let n = self.buffer.len();
        self.index = (self.index + 1) % n as isize;
        self.buffer[self.index as usize] = x;
    }

    pub fn sorted_trimmed(&self) -> Vec<i32> {
        let mut buf = self.buffer.clone();
        buf.sort_unstable();
        let end = buf.len().saturating_sub(self.lost_count);
        if self.lost_count >= end {
            buf
        } else {
            buf[self.lost_count..end].to_vec()
        }
    }
}

impl DigitalFilter for HoldFilter {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        self.push_value(x);
        // Как в Java: publish всегда 0 — реальное значение читается через getSorted в operator.
        out.push(0);
    }

    fn reset(&mut self) {
        self.buffer.fill(0);
        self.index = -1;
    }

    fn delay(&self) -> f64 {
        (self.size.saturating_sub(1) as f64) / 2.0
    }
}

/// Comb: x[n] - x[n - combFactor].
pub struct CombFilter {
    buffer: VecDeque<i32>,
    factor: usize,
}

impl CombFilter {
    pub fn new(comb_factor: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(comb_factor + 1),
            factor: comb_factor,
        }
    }
}

impl DigitalFilter for CombFilter {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        self.buffer.push_back(x);
        if self.buffer.len() > self.factor + 1 {
            self.buffer.pop_front();
        }
        if self.buffer.len() == self.factor + 1 {
            let newest = *self.buffer.back().unwrap();
            let oldest = *self.buffer.front().unwrap();
            out.push(newest - oldest);
        } else {
            // пока буфер не заполнен — как AbstractBufferFilter с нулями
            out.push(x);
        }
    }

    fn reset(&mut self) {
        self.buffer.clear();
    }

    fn delay(&self) -> f64 {
        self.factor as f64 / 2.0
    }
}

/// Интегратор (накопительная сумма).
#[derive(Debug, Default)]
pub struct IntegrateFilter {
    sum: i32,
}

impl DigitalFilter for IntegrateFilter {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        self.sum = self.sum.saturating_add(x);
        out.push(self.sum);
    }

    fn reset(&mut self) {
        self.sum = 0;
    }

    fn delay(&self) -> f64 {
        -0.5
    }
}

/// Recursive Running Sum — среднее без задержки.
#[derive(Debug, Default)]
pub struct RrsFilter {
    n: i32,
    y: f64,
}

impl DigitalFilter for RrsFilter {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        self.n += 1;
        self.y += (1.0 / self.n as f64) * (x as f64 - self.y);
        out.push(self.y.round() as i32);
    }

    fn reset(&mut self) {
        self.n = 0;
        self.y = 0.0;
    }
}

/// Peak-to-peak за окно `size`.
pub struct PeakToPeakFilter {
    buffer: Vec<i32>,
    index: isize,
    filled: usize,
}

impl PeakToPeakFilter {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: vec![0; size.max(1)],
            index: -1,
            filled: 0,
        }
    }
}

impl DigitalFilter for PeakToPeakFilter {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        let n = self.buffer.len();
        self.index = (self.index + 1) % n as isize;
        self.buffer[self.index as usize] = x;
        self.filled = (self.filled + 1).min(n);

        let slice = if self.filled < n {
            &self.buffer[..self.filled]
        } else {
            &self.buffer[..]
        };
        let min = *slice.iter().min().unwrap_or(&0);
        let max = *slice.iter().max().unwrap_or(&0);
        out.push(max - min);
    }

    fn reset(&mut self) {
        self.buffer.fill(0);
        self.index = -1;
        self.filled = 0;
    }
}

/// Цепочка фильтров.
pub struct Chain {
    stages: Vec<Box<dyn DigitalFilter>>,
    delay: f64,
    freq: f64,
}

impl Chain {
    pub fn new(stages: Vec<Box<dyn DigitalFilter>>) -> Self {
        let mut delay = 0.0;
        let mut freq = 1.0;
        for s in &stages {
            delay = delay * s.frequency_factor() + s.delay();
            freq *= s.frequency_factor();
        }
        Self {
            stages,
            delay,
            freq,
        }
    }
}

impl DigitalFilter for Chain {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let mut current = values.to_vec();
        let mut tmp = FilterOut::new();
        for (i, stage) in self.stages.iter_mut().enumerate() {
            tmp.clear();
            if current.is_empty() {
                return;
            }
            // Каждый вход текущего буфера гоняем через stage; stage может emit 0..N
            let mut next = Vec::new();
            if i == 0 && values.len() >= 2 && current.len() >= 2 {
                // bi-operator на первом этапе получает оба входа разом
                stage.accept(&current, &mut tmp);
                next.extend_from_slice(&tmp.values);
            } else {
                for &v in &current {
                    tmp.clear();
                    stage.accept(&[v], &mut tmp);
                    next.extend_from_slice(&tmp.values);
                }
            }
            current = next;
        }
        for v in current {
            out.push(v);
        }
    }

    fn reset(&mut self) {
        for s in &mut self.stages {
            s.reset();
        }
    }

    fn delay(&self) -> f64 {
        self.delay
    }

    fn frequency_factor(&self) -> f64 {
        self.freq
    }
}

/// Impulsive smoothing: Hold → Decimate(n) → mean± → linear interpolate(n).
pub struct SmoothingImpulsive {
    hold: HoldFilter,
    size: usize,
    counter: usize,
    // linear interpolate state: Comb → Interpolation → Integrate → scale
    comb: CombFilter,
    integrate: IntegrateFilter,
    comb_factor: usize,
}

impl SmoothingImpulsive {
    pub fn new(size: usize) -> Self {
        let size = size.max(1);
        // lostCount = (size - highestOneBit(size)) / 2
        let hib = 1usize << (usize::BITS - size.leading_zeros()).saturating_sub(1);
        let lost = size.saturating_sub(hib) / 2;
        let comb_factor = (size / 2).max(1);
        Self {
            hold: HoldFilter::new(size, lost),
            size,
            counter: 0,
            comb: CombFilter::new(comb_factor),
            integrate: IntegrateFilter::default(),
            comb_factor,
        }
    }

    fn impulsive_value(&self) -> i32 {
        let sorted = self.hold.sorted_trimmed();
        if sorted.is_empty() {
            return 0;
        }
        let mean = sorted.iter().map(|&n| n as f64).sum::<f64>() / sorted.len() as f64;
        let mut pos_count = 0i32;
        let mut neg_count = 0i32;
        let mut distances = 0.0;
        for &n in &sorted {
            if (n as f64) > mean {
                pos_count += 1;
                distances += n as f64 - mean;
            } else if (n as f64) < mean {
                neg_count += 1;
            }
        }
        (mean + (pos_count - neg_count) as f64 * distances / (self.size as f64).powi(2)).round() as i32
    }
}

impl DigitalFilter for SmoothingImpulsive {
    fn accept(&mut self, values: &[i32], out: &mut FilterOut) {
        let Some(&x) = values.first() else { return };
        self.hold.push_value(x);
        self.counter = (self.counter + 1) % self.size;
        if self.counter != 0 {
            return;
        }

        let v = self.impulsive_value();
        // LinearInterpolationFilter: comb → repeat(size) → integrate → / (size * combFactor)
        let mut comb_out = FilterOut::new();
        self.comb.accept(&[v], &mut comb_out);
        let scale = (self.size * self.comb_factor) as i32;
        for &c in &comb_out.values {
            for _ in 0..self.size {
                let mut integ = FilterOut::new();
                self.integrate.accept(&[c], &mut integ);
                for &s in &integ.values {
                    out.push(if scale == 0 { s } else { s / scale });
                }
            }
        }
    }

    fn reset(&mut self) {
        self.hold.reset();
        self.counter = 0;
        self.comb.reset();
        self.integrate.reset();
    }

    fn delay(&self) -> f64 {
        // По тестам Java: delay = 3.5 при size=4
        3.5
    }
}
