use super::protocol::{ProtocolParser, DataPacket};
use serialport::{SerialPort, SerialPortType};
use std::time::Duration;

pub struct ComPortReader {
    port: Option<Box<dyn SerialPort>>,
    parser: Box<dyn ProtocolParser>,
}

impl ComPortReader {
    pub fn new(parser: Box<dyn ProtocolParser>) -> Self {
        Self {
            port: None,
            parser,
        }
    }

    pub fn available_ports() -> Vec<String> {
        serialport::available_ports()
            .map(|ports| {
                ports
                    .into_iter()
                    .map(|port| port.port_name)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn connect(&mut self, port_name: &str, baud_rate: u32) -> Result<(), Box<dyn std::error::Error>> {
        let port = serialport::new(port_name, baud_rate)
            .timeout(Duration::from_millis(100))
            .open()?;

        self.port = Some(port);
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.port = None;
    }

    pub fn is_connected(&self) -> bool {
        self.port.is_some()
    }

    pub fn read_data(&mut self) -> Option<DataPacket> {
        if let Some(port) = &mut self.port {
            let mut buffer: Vec<u8> = vec![0; 1024];
            
            match port.read(buffer.as_mut_slice()) {
                Ok(bytes_read) if bytes_read > 0 => {
                    let data = &buffer[..bytes_read];
                    match self.parser.parse_data(data) {
                        Ok(packet) => Some(packet),
                        Err(_) => None,
                    }
                }
                _ => None,
            }
        } else {
            // Для тестирования генерируем тестовые данные
            Some(self.parser.generate_test_packet())
        }
    }
}