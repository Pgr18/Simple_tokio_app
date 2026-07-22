use std::time::{SystemTime, UNIX_EPOCH};

/// Декодированный кадр РКМ / РКМ-С.
/// Содержит только 5 реальных каналов; слоты 3, 6, 7, 9 и служебный 10-й не хранятся.
/// Частота дискретизации: 200 Гц на канал.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub timestamp: u64,
    pub rheo1: i32,
    pub base1: i32,
    pub ecg: i32,
    pub base2: i32,
    pub rheo2: i32,
}

/// Совместимое имя для UI / процессора данных.
pub type DataPacket = Frame;

impl Frame {
    pub fn new(rheo1: i32, base1: i32, ecg: i32, base2: i32, rheo2: i32) -> Self {
        Self {
            timestamp: now_ms(),
            rheo1,
            base1,
            ecg,
            base2,
            rheo2,
        }
    }

    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new(0, 0, 0, 0, 0)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Rheo1,
    Base1,
    Ecg,
    Base2,
    Rheo2,
}

impl ChannelType {
    pub fn is_bipolar(self) -> bool {
        matches!(self, Self::Rheo1 | Self::Ecg | Self::Rheo2)
    }

    /// Индекс пары в 20-байтном кадре (0..=9).
    pub fn pair_index(self) -> usize {
        match self {
            Self::Rheo1 => 0,
            Self::Base1 => 1,
            Self::Ecg => 3,
            Self::Base2 => 4,
            Self::Rheo2 => 7,
        }
    }
}
