//! Синхронизация 20-байтных кадров РКМ / РКМ-С.
//!
//! Как Java `AbstractFixedFrameBytesInterceptor` + `AbstractRcmBytesInterceptor.check`.
//! RCMS (`RcmsBytesInterceptor`): дополнительно `buffer[19] == 0`.

use std::collections::VecDeque;

pub const FRAME_SIZE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SyncError {
    NeedMoreData,
    InvalidFrame,
}

#[derive(Debug)]
pub struct FrameSynchronizer {
    incoming: VecDeque<u8>,
    window: [u8; FRAME_SIZE],
    filled: usize,
    /// RCMS: последний байт кадра должен быть 0.
    pub require_last_zero: bool,
    pub resync_count: u64,
}

impl Default for FrameSynchronizer {
    fn default() -> Self {
        Self {
            incoming: VecDeque::new(),
            window: [0u8; FRAME_SIZE],
            filled: 0,
            require_last_zero: false,
            resync_count: 0,
        }
    }
}

impl FrameSynchronizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_rcms() -> Self {
        Self {
            require_last_zero: true,
            ..Self::default()
        }
    }

    pub fn set_rcms(&mut self, rcms: bool) {
        self.require_last_zero = rcms;
    }

    pub fn clear(&mut self) {
        self.incoming.clear();
        self.window = [0u8; FRAME_SIZE];
        self.filled = 0;
    }

    pub fn is_locked(&self) -> bool {
        self.filled == FRAME_SIZE
    }

    pub fn buffer_len(&self) -> usize {
        self.incoming.len() + self.filled
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<[u8; FRAME_SIZE]> {
        self.incoming.extend(bytes);
        let mut frames = Vec::new();
        while let Some(b) = self.incoming.pop_front() {
            if let Some(frame) = self.push_one(b) {
                frames.push(frame);
            }
        }
        frames
    }

    pub fn flush(&mut self) -> Option<[u8; FRAME_SIZE]> {
        if self.filled == FRAME_SIZE && self.check(&self.window, 0) {
            let frame = self.window;
            self.window = [0u8; FRAME_SIZE];
            self.filled = 0;
            Some(frame)
        } else {
            None
        }
    }

    fn check(&self, buffer: &[u8; FRAME_SIZE], next: u8) -> bool {
        if self.require_last_zero && buffer[FRAME_SIZE - 1] != 0 {
            return false;
        }
        check_frame(buffer, next)
    }

    fn push_one(&mut self, b: u8) -> Option<[u8; FRAME_SIZE]> {
        if self.filled < FRAME_SIZE {
            self.window[self.filled] = b;
            self.filled += 1;
            return None;
        }

        if self.check(&self.window, b) {
            let frame = self.window;
            self.window = [0u8; FRAME_SIZE];
            self.window[0] = b;
            self.filled = 1;
            return Some(frame);
        }

        self.window.copy_within(1..FRAME_SIZE, 0);
        self.window[FRAME_SIZE - 1] = b;
        self.filled = FRAME_SIZE;
        self.resync_count += 1;
        None
    }
}

/// Java `AbstractRcmBytesInterceptor.check`.
pub fn check_frame(buffer: &[u8; FRAME_SIZE], next_frame_start: u8) -> bool {
    for i in 1..FRAME_SIZE {
        if buffer[i] == (i as u8 & 0x01) {
            return false;
        }
    }
    (buffer[0] & 0x01) == 0 && (next_frame_start & 0x01) == 0
}

pub fn validate_marker(frame: &[u8; FRAME_SIZE]) -> bool {
    frame[0] & 0x01 == 0
}

pub fn validate_frame(frame: &[u8; FRAME_SIZE]) -> bool {
    if !validate_marker(frame) {
        return false;
    }
    for i in (1..FRAME_SIZE).step_by(2) {
        if frame[i] & 0x01 != 0 {
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
        stream.push(make_frame(50)[0]);
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 50);
        assert_eq!(sync.resync_count, 0);
    }

    #[test]
    fn recovers_after_garbage_prefix() {
        let mut sync = FrameSynchronizer::new();
        let mut stream = vec![0x01, 0x03, 0x05, 0x07];
        stream.extend_from_slice(&make_frame(10));
        stream.extend_from_slice(&make_frame(11));
        stream.push(make_frame(12)[0]);
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], make_frame(10));
    }

    #[test]
    fn locked_tolerates_noisy_low_bit_on_other_channel() {
        let mut sync = FrameSynchronizer::new();
        let f0 = make_frame(1);
        let mut f1 = make_frame(2);
        f1[3] |= 1;
        if f1[3] == 1 {
            f1[3] = 3;
        }
        let mut stream = Vec::new();
        stream.extend_from_slice(&f0);
        stream.extend_from_slice(&f1);
        stream.push(make_frame(3)[0]);
        let frames = sync.push_bytes(&stream);
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn three_byte_marker_mid_stream() {
        let f0 = make_frame(1);
        let f1 = make_frame(2);
        let mut sync = FrameSynchronizer::new();
        let mut stream = Vec::new();
        stream.extend_from_slice(&f0);
        stream.extend_from_slice(&f1);
        stream.push(make_frame(3)[0]);
        assert_eq!(sync.push_bytes(&stream).len(), 2);
    }

    #[test]
    fn java_fixture_style_frame() {
        let frame: [u8; 20] = [
            0xf6, 0xdc, 0x83, 0xb8, 0xfb, 0xc4, 0x83, 0x84, 0x91, 0xa2, 0xf9, 0x9e, 0x81, 0x80,
            0xfb, 0xb2, 0x81, 0xf6, 0x81, 0x80,
        ];
        assert!(check_frame(&frame, 0xf6));
    }

    #[test]
    fn rcms_rejects_nonzero_tail() {
        let mut sync = FrameSynchronizer::new_rcms();
        let mut bad = make_frame(1);
        bad[19] = 0x02;
        let mut stream = Vec::new();
        stream.extend_from_slice(&bad);
        stream.extend_from_slice(&make_frame(2));
        stream.push(make_frame(3)[0]);
        let frames = sync.push_bytes(&stream);
        assert!(frames.iter().all(|f| f[19] == 0));
    }
}
