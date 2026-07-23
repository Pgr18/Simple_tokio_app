//! Синхронизация 20-байтных кадров РКМ / РКМ-С.
//!
//! Старт кадра: два подряд байта с bit0 == 0 (старший+младший РЕО-1).
//! После захвата («locked») проверяем только маркер пары РЕО-1 — иначе единичный
//! сбой bit0 на другом канале срывает синхронизацию на всём живом потоке.

use std::collections::VecDeque;

pub const FRAME_SIZE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SyncError {
    NeedMoreData,
    InvalidFrame,
}

#[derive(Debug, Default)]
pub struct FrameSynchronizer {
    buf: VecDeque<u8>,
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

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn buffer_len(&self) -> usize {
        self.buf.len()
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<[u8; FRAME_SIZE]> {
        self.buf.extend(bytes);
        let mut frames = Vec::new();
        while let Some(frame) = self.try_next_frame() {
            frames.push(frame);
        }
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
                // В lock — только маркер РЕО-1 (мягкая проверка).
                if validate_marker(&frame) {
                    self.drain(FRAME_SIZE);
                    return Some(frame);
                }
                self.locked = false;
                self.resync_count += 1;
                if !self.buf.is_empty() {
                    self.buf.pop_front();
                }
                continue;
            }

            let start = self.find_frame_start()?;
            if self.buf.len() < start + FRAME_SIZE {
                if start > 0 {
                    self.drain(start);
                }
                return None;
            }

            let frame = self.peek_frame(start)?;
            // При поиске — полная проверка младших байт.
            if validate_frame(&frame) {
                self.drain(start + FRAME_SIZE);
                self.locked = true;
                return Some(frame);
            }

            self.drain(start + 1);
            self.resync_count += 1;
        }
    }

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

/// Маркер начала: старший и младший байт РЕО-1 с bit0 == 0.
pub fn validate_marker(frame: &[u8; FRAME_SIZE]) -> bool {
    bit0_clear(frame[0]) && bit0_clear(frame[1])
}

/// Полная валидация при захвате синхронизации.
pub fn validate_frame(frame: &[u8; FRAME_SIZE]) -> bool {
    if !validate_marker(frame) {
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
                (seed & 0x7E) & !1
            } else {
                ((seed.wrapping_add(i as u8) << 1) & 0x7E) | 0x01
            };
            let low = ((seed.wrapping_add(i as u8 * 3) << 1) & 0x7E) & !1;
            f[i * 2] = high;
            f[i * 2 + 1] = low;
        }
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
        let mut stream = vec![0x01, 0x03, 0x05, 0x07];
        stream.extend_from_slice(&make_frame(10));
        stream.extend_from_slice(&make_frame(11));
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], make_frame(10));
    }

    #[test]
    fn locked_tolerates_noisy_low_bit_on_other_channel() {
        let mut sync = FrameSynchronizer::new();
        let f0 = make_frame(1);
        let mut f1 = make_frame(2);
        // После lock портим low BASE (offset 3) — bit0=1
        f1[3] |= 1;
        let mut stream = Vec::new();
        stream.extend_from_slice(&f0);
        stream.extend_from_slice(&f1);
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 2, "soft lock should keep second frame");
    }

    #[test]
    fn three_byte_marker_mid_stream() {
        let f0 = make_frame(1);
        let f1 = make_frame(2);
        let mut sync = FrameSynchronizer::new();
        let mut stream = Vec::new();
        stream.extend_from_slice(&f0);
        stream.extend_from_slice(&f1);
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 2);
    }
}
