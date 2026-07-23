//! Конфигурация COM-порта по JavaFX `AbstractRcmBytesInterceptor`:
//! `38400, 7 data bits, parity None, clearDTR`. RTS не трогаем.

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub clear_dtr: bool,
    pub timeout: Duration,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self::rcm_7n1(38400)
    }
}

impl SerialConfig {
    /// JavaFX: `DATA_BITS_7` + `CLEAR_DTR`, parity по умолчанию None.
    pub fn rcm_7n1(baud_rate: u32) -> Self {
        Self {
            baud_rate,
            data_bits: DataBits::Seven,
            parity: Parity::None,
            stop_bits: StopBits::One,
            clear_dtr: true,
            timeout: Duration::from_millis(10),
        }
    }

    /// Совместимое имя (раньше ошибочно называли 7O1).
    pub fn rcm_7o1(baud_rate: u32) -> Self {
        Self::rcm_7n1(baud_rate)
    }

    pub fn label(&self) -> String {
        format!("{} 7N1", self.baud_rate)
    }

    pub fn open(&self, port_name: &str) -> serialport::Result<Box<dyn SerialPort>> {
        let mut port = serialport::new(port_name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .flow_control(FlowControl::None)
            .timeout(self.timeout)
            .open()?;

        // Java: CLEAR_DTR. RTS не трогаем.
        if self.clear_dtr {
            let _ = port.write_data_terminal_ready(false);
        }
        Ok(port)
    }
}
