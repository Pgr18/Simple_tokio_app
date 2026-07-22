//! Синхронизация 20-байтных кадров РКМ / РКМ-С.
//!
//! Маркер начала кадра: три подряд байта с bit0 == 0; второй из тройки — начало кадра
//! (старший байт РЕО-1). У младших байт пар bit0 в норме всегда 0; у старшего байта
//! РЕО-1 bit0 принудительно сброшен. Алгоритм общий для базового РКМ и РКМ-С.

use std::collections::VecDeque;

pub const FRAME_SIZE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SyncError {
    /// Недостаточно байт для кадра / маркера.
    NeedMoreData,
    /// Кандидат не прошёл валидацию младших байт.
    InvalidFrame,
}

#[derive(Debug, Default)]
pub struct FrameSynchronizer {
    buf: VecDeque<u8>,
    /// После успешного кадра читаем следующие 20 байт без повторного поиска маркера.
    locked: bool,
    pub resync_count: u64,
}

impl FrameSynchronizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.locked = false;
    }

    pub fn buffer_len(&self) -> usize {
        self.buf.len()
    }

    /// Добавляет байты и возвращает все успешно собранные кадры.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<[u8; FRAME_SIZE]> {
        self.buf.extend(bytes);
        let mut frames = Vec::new();
        while let Some(frame) = self.try_next_frame() {
            frames.push(frame);
        }
        // Защита от раздувания буфера при полном мусоре в линии
        if self.buf.len() > 4096 {
            self.buf.clear();
            self.locked = false;
            self.resync_count += 1;
        }
        frames
    }

    fn try_next_frame(&mut self) -> Option<[u8; FRAME_SIZE]> {
        loop {
            if self.locked {
                if self.buf.len() < FRAME_SIZE {
                    return None;
                }
                let frame = self.peek_frame(0)?;
                if validate_frame(&frame) {
                    self.drain(FRAME_SIZE);
                    return Some(frame);
                }
                // Потеря синхронизации — ищем маркер заново
                self.locked = false;
                self.resync_count += 1;
                // сдвигаемся на 1 байт и продолжаем поиск
                if !self.buf.is_empty() {
                    self.buf.pop_front();
                }
                continue;
            }

            let start = self.find_frame_start()?;
            if self.buf.len() < start + FRAME_SIZE {
                // Маркер найден, но кадра ещё нет — ждём
                if start > 0 {
                    self.drain(start);
                }
                return None;
            }

            let frame = self.peek_frame(start)?;
            if validate_frame(&frame) {
                self.drain(start + FRAME_SIZE);
                self.locked = true;
                return Some(frame);
            }

            // Ложный маркер — сдвигаемся на 1 байт от кандидата
            self.drain(start + 1);
            self.resync_count += 1;
        }
    }

    /// Ищет начало кадра: два подряд байта с bit0==0 (старший+младший РЕО-1).
    /// Mid-stream это же место уникально как «второй из трёх» (хвост предыдущего кадра
    /// + high + low РЕО-1); ложные пары отсекаются `validate_frame`.
    fn find_frame_start(&self) -> Option<usize> {
        let len = self.buf.len();
        if len < 2 {
            return None;
        }
        for i in 0..=len - 2 {
            let a = *self.buf.get(i)?;
            let b = *self.buf.get(i + 1)?;
            if bit0_clear(a) && bit0_clear(b) {
                return Some(i);
            }
        }
        None
    }

    fn peek_frame(&self, start: usize) -> Option<[u8; FRAME_SIZE]> {
        if self.buf.len() < start + FRAME_SIZE {
            return None;
        }
        let mut frame = [0u8; FRAME_SIZE];
        for (i, slot) in frame.iter_mut().enumerate() {
            *slot = *self.buf.get(start + i)?;
        }
        Some(frame)
    }

    fn drain(&mut self, n: usize) {
        for _ in 0..n.min(self.buf.len()) {
            self.buf.pop_front();
        }
    }
}

#[inline]
fn bit0_clear(b: u8) -> bool {
    b & 0x01 == 0
}

/// Кадр валиден, если bit0 старшего байта РЕО-1 == 0 и у всех младших байт пар bit0 == 0.
pub fn validate_frame(frame: &[u8; FRAME_SIZE]) -> bool {
    if !bit0_clear(frame[0]) {
        return false;
    }
    for i in (1..FRAME_SIZE).step_by(2) {
        if !bit0_clear(frame[i]) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(seed: u8) -> [u8; 20] {
        let mut f = [0u8; 20];
        for i in 0..10 {
            let high = if i == 0 {
                // маркер: bit0 = 0, данные из seed
                (seed & 0x7E) & !1
            } else {
                // старшие байты: bit0 = 1 (как в базовом РКМ)
                ((seed.wrapping_add(i as u8) << 1) & 0x7E) | 0x01
            };
            let low = ((seed.wrapping_add(i as u8 * 3) << 1) & 0x7E) & !1; // bit0 = 0
            f[i * 2] = high;
            f[i * 2 + 1] = low;
        }
        // служебный канал 10 как в РКМ-С
        f[18] = 0x01;
        f[19] = 0x00;
        debug_assert!(validate_frame(&f));
        f
    }

    #[test]
    fn syncs_aligned_stream_without_loss() {
        let mut sync = FrameSynchronizer::new();
        let mut stream = Vec::new();
        for n in 0..50u8 {
            stream.extend_from_slice(&make_frame(n));
        }
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 50);
        assert_eq!(sync.resync_count, 0);
        assert!(sync.buf.is_empty());
    }

    #[test]
    fn recovers_after_garbage_prefix() {
        let mut sync = FrameSynchronizer::new();
        let mut stream = vec![0x01, 0x03, 0x05, 0x07]; // bit0=1 — мусор
        stream.extend_from_slice(&make_frame(10));
        stream.extend_from_slice(&make_frame(11));
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], make_frame(10));
    }

    #[test]
    fn three_byte_marker_mid_stream() {
        let f0 = make_frame(1);
        let f1 = make_frame(2);
        // На стыке кадров: low последнего канала f0 и high+low РЕО-1 f1 — три bit0==0
        assert_eq!(f0[19] & 1, 0);
        assert_eq!(f1[0] & 1, 0);
        assert_eq!(f1[1] & 1, 0);

        let mut sync = FrameSynchronizer::new();
        let mut stream = Vec::new();
        stream.extend_from_slice(&f0);
        stream.extend_from_slice(&f1);
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 2);
    }
}
