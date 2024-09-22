use serial2_tokio::SerialPort;
use std::io::Error;

#[tokio::main]
pub async fn scan() -> Result<(),()> {
let ports = SerialPort::available_ports().expect("No ports found!");
for p in ports {
    println!("{}", p.display().to_string());
}
Ok(())
}

pub async fn connect(user_path : &str) -> Result<SerialPort,Error> {
    let port = SerialPort::open(user_path, 921600)?;
    Ok(port)
    }
