#[derive(Debug, Clone)]
pub struct DataPacket {
    pub timestamp: u64,           // Временная метка
    pub rheocardiogram: i16,      // Реокардиограмма (знаковое)
    pub base_impedance: u16,      // Базовая импеданс (беззнаковое)
    pub ecg: i16,                 // ECG (знаковое)
    pub channel4: u16,            // Зарезервированный канал 4
    pub channel5: u16,            // Зарезервированный канал 5
    pub channel6: u16,            // Зарезервированный канал 6
    pub channel7: u16,            // Зарезервированный канал 7
    pub channel8: u16,            // Зарезервированный канал 8
}

pub trait ProtocolParser: Send + Sync {
    fn parse_data(&self, raw_data: &[u8]) -> Result<DataPacket, ProtocolError>;
    fn generate_test_packet(&self) -> DataPacket;
}

#[derive(Debug)]
pub struct MedicalProtocolParser;

impl MedicalProtocolParser {
    /// Преобразует 12-битное значение в i16 (с учетом знака)
    fn parse_12bit_signed(low_byte: u8, high_byte: u8) -> i16 {
        // Бит D11 (бит 11) - знаковый бит
        let mut value = ((high_byte as u16 & 0x0F) << 6) | (low_byte as u16 & 0x3F);
        
        // Если знаковый бит установлен (D11 = 1), значение отрицательное
        if (high_byte & 0x08) != 0 {
            // Расширяем знак для 12-битного числа
            if value > 0x07FF {
                value = value | 0xF000; // Расширяем знак для 16-битного представления
            }
        }
        value as i16
    }
    
    /// Преобразует 12-битное значение в u16 (беззнаковое)
    fn parse_12bit_unsigned(low_byte: u8, high_byte: u8) -> u16 {
        ((high_byte as u16 & 0x0F) << 6) | (low_byte as u16 & 0x3F)
    }
    
    /// Парсит одну посылку (7 бит данных + 1 бит флага)
    fn parse_parcel(byte: u8) -> (u8, bool) {
        let data = byte >> 1; // 7 старших бит - данные
        let flag = (byte & 0x01) != 0; // Младший бит - флаг
        (data, flag)
    }
}

impl ProtocolParser for MedicalProtocolParser {
    fn parse_data(&self, raw_data: &[u8]) -> Result<DataPacket, ProtocolError> {
        // Для полного блока нужно 20 байт (20 посылок)
        const BLOCK_SIZE: usize = 20;
        
        if raw_data.len() < BLOCK_SIZE {
            return Err(ProtocolError::IncompleteData);
        }
        
        let mut parcels = Vec::with_capacity(BLOCK_SIZE);
        
        // Парсим все посылки
        for &byte in &raw_data[..BLOCK_SIZE] {
            let (data, flag) = Self::parse_parcel(byte);
            parcels.push((data, flag));
        }
        
        // Проверяем структуру блока
        // Первая посылка должна быть high bits (флаг=0) для рео
        if parcels[0].1 != false {
            return Err(ProtocolError::InvalidFormat);
        }
        
        // Вторая посылка должна быть low bits (флаг=0) для канала 1
        if parcels[1].1 != false {
            return Err(ProtocolError::InvalidFormat);
        }
        
        // Третья посылка должна быть high bits (флаг=1) для базового импеданса
        if parcels[2].1 != true {
            return Err(ProtocolError::InvalidFormat);
        }
        
        // Четвертая посылка должна быть low bits (флаг=0) для канала 2
        if parcels[3].1 != false {
            return Err(ProtocolError::InvalidFormat);
        }
        
        // Седьмая посылка должна быть high bits (флаг=1) для ECG
        if parcels[6].1 != true {
            return Err(ProtocolError::InvalidFormat);
        }
        
        // Восьмая посылка должна быть low bits (флаг=0) для канала 3
        if parcels[7].1 != false {
            return Err(ProtocolError::InvalidFormat);
        }
        
        // Парсим данные каналов
        // 1. Реокардиограмма (канал 1) - знаковое
        let rheo_high = parcels[0].0; // Посылка 1: D6-D11 + sign
        let rheo_low = parcels[1].0;  // Посылка 2: D0-D5
        let rheocardiogram = Self::parse_12bit_signed(rheo_low, rheo_high);
        
        // 2. Базовая импеданс (канал 2) - беззнаковое
        let imp_high = parcels[2].0; // Посылка 3: D6-D11
        let imp_low = parcels[3].0;  // Посылка 4: D0-D5
        let base_impedance = Self::parse_12bit_unsigned(imp_low, imp_high);
        
        // 3. ECG (канал 3) - знаковое
        let ecg_high = parcels[6].0; // Посылка 7: D6-D11 + sign
        let ecg_low = parcels[7].0;  // Посылка 8: D0-D5
        let ecg = Self::parse_12bit_signed(ecg_low, ecg_high);
        
        // 4-8. Зарезервированные каналы (беззнаковые)
        let ch4_high = parcels[8].0;  // Посылка 9
        let ch4_low = parcels[9].0;   // Посылка 10
        let channel4 = Self::parse_12bit_unsigned(ch4_low, ch4_high);
        
        let ch5_high = parcels[10].0; // Посылка 11
        let ch5_low = parcels[11].0;  // Посылка 12
        let channel5 = Self::parse_12bit_unsigned(ch5_low, ch5_high);
        
        let ch6_high = parcels[12].0; // Посылка 13
        let ch6_low = parcels[13].0;  // Посылка 14
        let channel6 = Self::parse_12bit_unsigned(ch6_low, ch6_high);
        
        let ch7_high = parcels[14].0; // Посылка 15
        let ch7_low = parcels[15].0;  // Посылка 16
        let channel7 = Self::parse_12bit_unsigned(ch7_low, ch7_high);
        
        // Для канала 8 используем нули (нет данных в протоколе)
        let channel8 = 0;


        
        Ok(DataPacket {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            rheocardiogram,
            base_impedance,
            ecg,
            channel4,
            channel5,
            channel6,
            channel7,
            channel8,
        })
    }

    fn generate_test_packet(&self) -> DataPacket {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Генерируем тестовые данные
        let time_sec = timestamp as f64 / 1000.0;
        
        DataPacket {
            timestamp,
            rheocardiogram: (time_sec.sin() * 1000.0) as i16,     // Синусоида
            base_impedance: 500 + ((time_sec * 0.5).sin() * 100.0) as u16, // Медленная синусоида
            ecg: (time_sec * 10.0).sin() as i16 * 100,           // Быстрая синусоида (ECG-like)
            channel4: 1000,
            channel5: 2000,
            channel6: 3000,
            channel7: 4000,
            channel8: 0,
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