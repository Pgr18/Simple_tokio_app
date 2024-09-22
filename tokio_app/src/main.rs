//use mini_redis::{client,Result};
pub mod serial;

use std::any::type_name;


fn type_of<T>(_: T) -> &'static str {
    type_name::<T>()
}


fn main()  {
    let _= serial::scan();
    let _ok_port= serial::connect("COM3");
    if type_of(_ok_port) == "SerialPort" {
        println!("OK");
    } else {
        println!("Error");
    }

    
}
