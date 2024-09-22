use serial2_tokio::SerialPort;
#[tokio::main]
pub async fn main() -> Result<(),()> {
// On Windows, use something like "COM1" or "COM15".
let ports = SerialPort::available_ports().expect("No ports found!");
for p in ports {
    println!("{}", p.display().to_string());
}
Ok(())
}