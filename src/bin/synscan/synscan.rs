use astrotools::AstroSerialDevice;
use hex::FromHex;
use lightspeed_astro::devices::actions::DeviceActions;
use lightspeed_astro::props::Property;
use log::{debug, error, info, warn};
#[cfg(windows)]
use serialport::COMPort;
#[cfg(unix)]
use serialport::TTYPort;
use serialport::{available_ports, SerialPortType, UsbPortInfo};
use skywatcher_rs::str_24bits_to_u32;
use skywatcher_rs::{degrees_to_precise_revolutions, degrees_to_revolutions};
use std::fmt::UpperHex;
use std::io::{Read, Write};
use std::time::Duration;
use uuid::Uuid;

enum Command {
    Echo = 0x4b,
    GetRaDec = 0x45,
    GetPreciseRaDec = 0x65,
    GetAltAz = 0x5a,
    GetPreciseAltAz = 0x7a,
    GoToRaDec = 0x52,
    GoToPreciseRaDec = 0x72,
    GoToAltAz = 0x42,
    GoToPreciseAltAz = 0x62,
}

pub struct MountDevice {
    id: Uuid,
    name: String,
    pub properties: Vec<Property>,
    address: String,
    pub baud: u32,
    #[cfg(unix)]
    pub port: TTYPort,
    #[cfg(windows)]
    pub port: COMPort,
}

impl AstroSerialDevice for MountDevice {
    fn new(name: &str, address: &str, baud: u32, timeout_ms: u64) -> Option<Self> {
        let builder = serialport::new(address, baud).timeout(Duration::from_millis(timeout_ms));

        if let Ok(port_) = builder.open_native() {
            let mut dev = Self {
                id: Uuid::new_v4(),
                name: name.to_owned(),
                properties: Vec::new(),
                address: address.to_owned(),
                baud,
                port: port_,
            };

            if let Err(e) = dev.send_command(Command::Echo as i32, Some("x".to_string())) {
                debug!("Cannot connect to mount after command: {}", e as i32);
                return None;
            }

            dev.init_device();
            dev.fetch_props();
            Some(dev)
        } else {
            debug!("Cannot connect to mount - unknonw");
            None
        }
    }

    fn fetch_props(&mut self) {
        info!("Fetching props");
    }

    fn get_id(&self) -> Uuid {
        self.id
    }

    fn get_address(&self) -> &String {
        &self.address
    }

    fn get_name(&self) -> &String {
        &self.name
    }

    fn get_properties(&self) -> &Vec<Property> {
        &self.properties
    }

    fn send_command<T>(&mut self, comm: T, val: Option<String>) -> Result<String, DeviceActions>
    where
        T: UpperHex,
    {
        // First convert the command into an hex STRING
        let mut hex_command = format!("{:X}", comm);

        if let Some(value) = val {
            hex_command += hex::encode(value).as_str();
        }
        debug!("Hex command: {:?}", &hex_command);
        // Cast the hex string to a sequence of bytes
        let command: Vec<u8> = Vec::from_hex(hex_command).expect("Invalid Hex String");

        match self.port.write(&command) {
            Ok(_) => {
                debug!("Sent command: {}", std::str::from_utf8(&command).unwrap());
                let mut final_buf: Vec<u8> = Vec::new();
                debug!("Receiving data");

                loop {
                    let mut read_buf = [0; 1];

                    match self.port.read(read_buf.as_mut_slice()) {
                        Ok(_) => {
                            let byte = read_buf[0];
                            //debug!("Read byte: {}", byte);
                            final_buf.push(byte);

                            if byte == 0x23 as u8 {
                                break;
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                            error!("Timeout");
                            return Err(DeviceActions::Timeout);
                        }
                        Err(e) => error!("Unknown error occurred {:?}", e),
                    }
                }

                // Use this to check if the response is OK (=) or there is an error (!)
                let response = std::str::from_utf8(&final_buf).unwrap();
                debug!("RESPONSE: {}", response);
                Ok(response.to_owned())
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => Err(DeviceActions::Timeout),
            Err(e) => {
                error!("{:?}", e);
                Err(DeviceActions::ComError)
            }
        }
    }

    fn update_property(&mut self, _: &str, _: &str) -> Result<(), DeviceActions> {
        todo!()
    }

    fn update_property_remote(&mut self, _: &str, _: &str) -> Result<(), DeviceActions> {
        todo!()
    }
    fn find_property_index(&self, _: &str) -> Option<usize> {
        todo!()
    }
}

trait SynScanMount {
    fn init_device(&mut self);
    fn echo(&mut self, val: String);
    fn get_ra_dec_position(&mut self) -> String;
    fn get_precise_ra_dec_position(&mut self) -> String;
    fn get_alt_az_position(&mut self) -> String;
    fn get_precise_alt_az_position(&mut self) -> String;
    fn goto_ra_dec(&mut self, ra_degrees: f32, dec_degrees: f32);
    fn goto_precise_ra_dec(&mut self, ra_degrees: f32, dec_degrees: f32);
    fn goto_alt_az(&mut self, degrees: f32);
    fn goto_precise_alt_az(&mut self, degrees: f32);
}

impl SynScanMount for MountDevice {
    fn init_device(&mut self) {
        self.get_ra_dec_position();
        self.get_precise_ra_dec_position();
        self.get_alt_az_position();
        self.get_precise_alt_az_position();
        //self.goto_precise_ra_dec(26.251938, 90.00011);
        self.goto_precise_ra_dec(0.0000, 90.00011);
    }

    /// Useful for debugging or to check communication
    /// with the mount
    fn echo(&mut self, val: String) {
        let _version = match self.send_command(Command::Echo as i32, Some(val)) {
            Ok(v) => v,
            Err(_) => "UNKNONW".to_string(),
        };
    }

    fn get_ra_dec_position(&mut self) -> String {
        match self.send_command(Command::GetRaDec as i32, None) {
            Ok(p) => p,
            Err(_) => String::from("UNKNOWN"),
        }
    }

    fn get_precise_ra_dec_position(&mut self) -> String {
        match self.send_command(Command::GetPreciseRaDec as i32, None) {
            Ok(p) => p,
            Err(_) => String::from("UNKNOWN"),
        }
    }

    fn get_alt_az_position(&mut self) -> String {
        match self.send_command(Command::GetAltAz as i32, None) {
            Ok(p) => p,
            Err(_) => String::from("UNKNOWN"),
        }
    }

    fn get_precise_alt_az_position(&mut self) -> String {
        match self.send_command(Command::GetPreciseAltAz as i32, None) {
            Ok(p) => p,
            Err(_) => String::from("UNKNOWN"),
        }
    }

    fn goto_ra_dec(&mut self, ra_degrees: f32, dec_degrees: f32) {
        let dec_revolutions = degrees_to_revolutions(dec_degrees);
        let ra_revolutions = degrees_to_revolutions(ra_degrees);
        debug!("DEC rev calculated: {}", dec_revolutions);
        debug!("RA rev calculated: {}", ra_revolutions);

        let payload = format!(
            "{},{}",
            format!("{:04X}", ra_revolutions),
            format!("{:04X}", dec_revolutions)
        );
        debug!("GOTO payload: {}", &payload);
        self.send_command(Command::GoToRaDec as i32, Some(payload));
    }
    fn goto_precise_ra_dec(&mut self, ra_degrees: f32, dec_degrees: f32) {
        let dec_revolutions = degrees_to_precise_revolutions(dec_degrees);
        let ra_revolutions = degrees_to_precise_revolutions(ra_degrees);
        debug!("DEC rev calculated: {}", dec_revolutions);
        debug!("RA rev calculated: {}", ra_revolutions);

        let payload = format!(
            "{},{}",
            format!("{:8X}", ra_revolutions << 8),
            format!("{:8X}", dec_revolutions << 8),
        );
        debug!("precise GOTO payload: {}", &payload);
        self.send_command(Command::GoToPreciseRaDec as i32, Some(payload));
    }

    fn goto_alt_az(&mut self, degrees: f32) {}
    fn goto_precise_alt_az(&mut self, degrees: f32) {}
}

pub fn look_for_devices() -> Vec<(String, UsbPortInfo)> {
    let ports = available_ports().unwrap();
    let mut devices = Vec::new();

    for port in ports {
        if let SerialPortType::UsbPort(info) = port.port_type {
            if info.vid == 0x067b && info.pid == 0x2303 {
                devices.push((port.port_name, info));
            }
        }
    }

    match devices.len() {
        0 => warn!("No Sky-Watcher mount found"),
        n => info!("Found {} Sky-Watcher mount(s)", n),
    }

    devices
}
