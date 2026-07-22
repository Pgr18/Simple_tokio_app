//! Конфигурация и открытие COM-порта для реографа РКМ / РКМ-С.
//!
//! Настройки по умолчанию взяты из документации базового РКМ.
//! Для модификации РКМ-С отдельная конфигурация не задокументирована —
//! используем те же значения как рабочее предположение.

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::time::Duration;

/// Параметры RS-232 для РКМ / РКМ-С.
///
/// Предположение по умолчанию (документировано для базового РКМ;
/// для РКМ-С отдельной спецификации нет — те же значения):
/// `38400, 8 бит, без чётности, 1 стоп-бит, RTS=true, DTR=false`.
#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub rts: bool,
    pub dtr: bool,
    pub timeout: Duration,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            baud_rate: 38400,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            rts: true,
            dtr: false,
            timeout: Duration::from_millis(50),
        }
    }
}

impl SerialConfig {
    pub fn open(&self, port_name: &str) -> serialport::Result<Box<dyn SerialPort>> {
        let mut port = serialport::new(port_name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .flow_control(FlowControl::None)
            .timeout(self.timeout)
            .open()?;

        // RTS / DTR выставляются один раз при открытии и не меняются.
        port.write_request_to_send(self.rts)?;
        port.write_data_terminal_ready(self.dtr)?;
        Ok(port)
    }
}
