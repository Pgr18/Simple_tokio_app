use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct DataPacket {
    pub timestamp: u64,
    pub channel_a: f64,
    pub channel_b: f64,
    pub channel_c: f64,
    pub status: u8,
}

pub trait ProtocolParser: Send + Sync {
    fn parse_data(&self, raw_data: &[u8]) -> Result<DataPacket, ProtocolError>;
    fn generate_test_packet(&self) -> DataPacket;
}

#[derive(Debug)]
pub struct SimpleProtocolParser;

impl ProtocolParser for SimpleProtocolParser {
    fn parse_data(&self, raw_data: &[u8]) -> Result<DataPacket, ProtocolError> {
        if raw_data.len() < 22 {
            return Err(ProtocolError::IncompleteData);
        }

        if raw_data[0] != 0xAA || raw_data[21] != 0x55 {
            return Err(ProtocolError::InvalidFormat);
        }

        let timestamp = u64::from_be_bytes([
            raw_data[1], raw_data[2], raw_data[3], raw_data[4],
            raw_data[5], raw_data[6], raw_data[7], raw_data[8],
        ]);

        let channel_a = f32::from_be_bytes([raw_data[9], raw_data[10], raw_data[11], raw_data[12]]) as f64;
        let channel_b = f32::from_be_bytes([raw_data[13], raw_data[14], raw_data[15], raw_data[16]]) as f64;
        let channel_c = f32::from_be_bytes([raw_data[17], raw_data[18], raw_data[19], raw_data[20]]) as f64;

        Ok(DataPacket {
            timestamp,
            channel_a,
            channel_b,
            channel_c,
            status: raw_data[21],
        })
    }

    fn generate_test_packet(&self) -> DataPacket {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        DataPacket {
            timestamp,
            channel_a: (timestamp as f64 * 0.01).sin(),
            channel_b: (timestamp as f64 * 0.02).cos(),
            channel_c: (timestamp as f64 * 0.005).sin() * 2.0,
            status: 0x01,
        }
    }
}

#[derive(Debug)]
pub enum ProtocolError {
    IncompleteData,
    InvalidFormat,
    ChecksumError,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::IncompleteData => write!(f, "Incomplete data received"),
            ProtocolError::InvalidFormat => write!(f, "Invalid data format"),
            ProtocolError::ChecksumError => write!(f, "Checksum error"),
        }
    }
}

impl std::error::Error for ProtocolError {}