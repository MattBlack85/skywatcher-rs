use astrotools::AstroSerialDevice;
use hex::FromHex;
use lightspeed_astro::devices::actions::DeviceActions;
use lightspeed_astro::props::Permission;
use lightspeed_astro::props::Property;
use log::{debug, error, info, warn};
#[cfg(windows)]
use serialport::COMPort;
#[cfg(unix)]
use serialport::TTYPort;
use serialport::{available_ports, SerialPortType, UsbPortInfo};
use skywatcher_rs::{
    degrees_to_precise_revolutions, degrees_to_revolutions, str_24bits_to_u32, TrackingMode,
};
use skywatcher_rs::{precise_revolutions_to_degrees, str_to_u32};
use std::fmt::UpperHex;
use std::io::{Read, Write};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use runiverse::transform::{dec_to_deg, ra_to_deg};
use runiverse::{Declination, RightAscension};
use uuid::Uuid;

const TRACKING_OFF: &str = "Off";
const TRACKING_ALT_AZ: &str = "AltAz";
const TRACKING_EQUATORIAL: &str = "Equatorial";
const TRACKING_PEC: &str = "PEC";

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
    GetTrackingMode = 0x74,
    SetTrackingMode = 0x54,
    GetVersion = 0x56,
    GetModel = 0x6d,
    GetAlignment = 0x4a,
}

pub struct CustomProp {
    name: String,
    value: Arc<RwLock<String>>,
    kind: String,
    permission: Permission,
}

impl CustomProp {
    fn to_ls_prop(&self) -> Property {
        Property {
            name: self.name.to_string(),
            value: self.value.read().unwrap().to_string(),
            kind: self.kind.to_string(),
            permission: self.permission as i32,
        }
    }
}

pub struct MountDevice {
    id: Uuid,
    name: String,
    properties: Vec<CustomProp>,
    static_properties: Vec<Property>,
    address: String,
    pub baud: u32,
    #[cfg(all(unix, not(test)))]
    pub port: TTYPort,
    #[cfg(all(windows, not(test)))]
    pub port: COMPort,
    #[cfg(test)]
    pub port: MockableSerial,
    track_mode: Arc<RwLock<String>>,
    aligned: Arc<RwLock<String>>,
}

use std::io::{Error, ErrorKind};

pub struct MockableSerial {
    next_success: bool,
    next_response: Vec<u8>,
    last_read: usize
}

impl MockableSerial {
    fn new(address: &str, baud: u32) -> Self {
        Self {
            next_success: true,
            next_response: vec!(0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x23),
	    last_read: 0
        }
    }

    fn open_native(&self) -> Result<Self, std::io::Error> {
        Ok(
	    Self {
		next_success: true,
		next_response: vec!(0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x23),
		last_read: 0
            }
	)
    }

    fn write(&self, _b: &Vec<u8>) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn read(&mut self, buff: &mut [u8]) -> Result<(), std::io::Error> {
        let v = *self.next_response.get(self.last_read).unwrap();
	buff[0] = v;

	debug!("Index is at: {}", self.last_read);
	if v == 0x23 {
	    self.last_read = 0;
	} else {
	    self.last_read += 1;
	}
	debug!("After Index is at: {}", self.last_read);

        if self.next_success {
            return Ok(());
        } else {
            return Err(Error::new(ErrorKind::Other, "An error"));
        };
    }
}

pub struct SerialType<T> {
    pub st: T
}

#[cfg(test)]
fn get_serial_port(address: &str, baud: u32, timeout_ms: u64) -> SerialType<MockableSerial> {
    SerialType { st: MockableSerial::new("/dev/abc", 9600) }
}

#[cfg(not(test))]
fn get_serial_port(address: &str, baud: u32, timeout_ms: u64) -> SerialType<serialport::SerialPortBuilder> {
    SerialType { st: serialport::new(address, baud).timeout(Duration::from_millis(timeout_ms)) }
}

impl AstroSerialDevice for MountDevice {
    
    
    fn new(name: &str, address: &str, baud: u32, timeout_ms: u64) -> Option<Self> {
	#[cfg(not(test))]
	let builder: SerialType<serialport::SerialPortBuilder> = get_serial_port(address, baud, timeout_ms);

	#[cfg(test)]
	let builder: SerialType<MockableSerial> = get_serial_port(address, baud, timeout_ms);

        if let Ok(port_) = builder.st.open_native() {
            let mut dev = Self {
                id: Uuid::new_v4(),
                name: name.to_owned(),
                properties: Vec::new(),
                static_properties: Vec::new(),
                address: address.to_owned(),
                baud,
                port: port_,
                track_mode: Arc::new(RwLock::new(String::from("Off"))),
                aligned: Arc::new(RwLock::new(String::from("false"))),
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
        info!("Fetching actual state");
        self.get_tracking_mode();
        self.get_precise_ra_dec_position();
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
        todo!()
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
        debug!("Sent RAW command: {:?}", &command);

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
                            println!("Read byte: {}", byte);
                            final_buf.push(byte);

                            if byte == 0x23 as u8 {
				println!("Breaking");
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
                debug!("RAW RESPONSE: {:?}", &final_buf);
                // Use this to check if the response is OK (=) or there is an error (!)
                let response = String::from_utf8(final_buf).unwrap();
                debug!("RESPONSE: {}", response);
                Ok(response)
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => Err(DeviceActions::Timeout),
            Err(e) => {
                error!("{:?}", e);
                Err(DeviceActions::ComError)
            }
        }
    }

    fn update_property(&mut self, name: &str, value: &str) -> Result<(), DeviceActions> {
        info!("Synscan updating property {} with {}", name, value);
        if let Some(prop_idx) = self.find_property_index(name) {
            let r_prop = self.properties.get(prop_idx).unwrap();

            match r_prop.permission {
                Permission::ReadOnly => Err(DeviceActions::CannotUpdateReadOnlyProperty),
                _ => self.update_property_remote(name, value),
            }
        } else {
            Err(DeviceActions::UnknownProperty)
        }
    }

    fn update_property_remote(&mut self, name: &str, value: &str) -> Result<(), DeviceActions> {
        match name {
            "TRACKING_MODE" => self.set_tracking_mode(value),
            _ => Err(DeviceActions::UnknownProperty),
        }
    }
    fn find_property_index(&self, name: &str) -> Option<usize> {
        let mut index = 256;

        for (idx, prop) in self.properties.iter().enumerate() {
            if prop.name == name {
                index = idx;
                break;
            }
        }
        if index == 256 {
            None
        } else {
            Some(index)
        }
    }
}

pub trait SynScanMount {
    fn init_device(&mut self);
    fn echo(&mut self, val: String);
    fn get_ra_dec_position(&mut self) -> String;
    fn get_precise_ra_dec_position(&mut self) -> String;
    fn get_alt_az_position(&mut self) -> String;
    fn get_precise_alt_az_position(&mut self) -> String;
    fn goto_ra_dec(&mut self, ra_degrees: f32, dec_degrees: f32);
    fn goto_precise_ra_dec(&mut self, ra_degrees: f64, dec_degrees: f64);
    fn goto_alt_az(&mut self, degrees: f32);
    fn goto_precise_alt_az(&mut self, degrees: f32);
    fn get_tracking_mode(&mut self);
    fn set_tracking_mode(&mut self, mode: &str) -> Result<(), DeviceActions>;
    fn init_props(&mut self);
    fn get_ls_props(&self) -> Vec<Property>;
    fn get_version(&mut self) -> String;
    fn get_model(&mut self) -> String;
    fn is_aligned(&mut self);
}

impl SynScanMount for MountDevice {
    fn get_ls_props(&self) -> Vec<Property> {
        let mut ls_props = Vec::with_capacity(self.properties.len() + self.static_properties.len());
        for p in &self.properties {
            info!("Transforming into LS PROP: {}", p.name);
            ls_props.push(p.to_ls_prop());
        }
        for p in &self.static_properties {
            ls_props.push(p.clone());
        }

        ls_props
    }
    fn init_device(&mut self) {
        self.get_ra_dec_position();
        self.get_precise_ra_dec_position();
        self.get_alt_az_position();
        self.get_precise_alt_az_position();
        self.get_version();
        self.init_props();
        // let ra = RightAscension::new(17, 41, 56.35);
        // let dec = Declination::new(72, 8, 55.86);
        // let ra_deg = ra_to_deg(&ra);
        // let dec_deg = dec_to_deg(&dec);

        // info!("RA degrees: {} DEC degrees: {}", ra_deg, dec_deg);

        // self.goto_precise_ra_dec(ra_deg, dec_deg);
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
            Ok(p) => {
                let ra_rev = (&p[0..6]).to_string();
                let dec_rev = (&p[9..15]).to_string();
                info!("RA rev,DEC rev: {} {}", &ra_rev, &dec_rev);
                let ra_rev_s = str_to_u32(ra_rev).unwrap();
                let dec_rev_s = str_to_u32(dec_rev).unwrap();
                info!(
                    "RA rev integer,DEC rev integer: {} {}",
                    &ra_rev_s, &dec_rev_s
                );
                let ra = precise_revolutions_to_degrees(ra_rev_s);
                let dec = precise_revolutions_to_degrees(dec_rev_s);
                info!("RA degrees,DEC degrees: {} {}", &ra, &dec);
		let real_dec = Declination::from_degrees(dec as f64);
		info!("Dec: {}", real_dec);
                p
            }
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
    fn goto_precise_ra_dec(&mut self, ra_degrees: f64, dec_degrees: f64) {
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

    fn get_tracking_mode(&mut self) {
        let new_tm = match self.send_command(Command::GetTrackingMode as i32, None) {
            Ok(t) => match t.as_str() {
                "\0#" => TRACKING_OFF.to_string(),
                "\u{1}#" => TRACKING_ALT_AZ.to_string(),
                "\u{2}#" => TRACKING_EQUATORIAL.to_string(),
                "\u{3}#" => TRACKING_PEC.to_string(),
                _ => String::from("UNKNOWN"),
            },
            Err(_) => {
                error!("Couldn't read actual tracking mode of the mount");
                String::from("-1")
            }
        };

        let mut tm = self.track_mode.write().unwrap();

        if tm.to_string() != new_tm {
            info!("GET => Updating track mode");
            tm.clear();
            tm.push_str(&new_tm.to_owned());
            info!("GET => Updating track mode");
        }
    }

    fn set_tracking_mode(&mut self, mode: &str) -> Result<(), DeviceActions> {
        let mode_code = match mode {
            "Off" => "\0",
            "AltAz" => "\u{1}",
            "Equatorial" => "\u{2}",
            "PEC" => "\u{3}",
            _ => {
                error!("Tracking mode: {} not supported", mode);
                "UNKNOWN"
            }
        };
        warn!("CODE: {:?}", mode_code.as_bytes());

        if mode_code == "UNKNOWN" {
            return Err(DeviceActions::InvalidValue);
        }

        let old_tm = self.track_mode.read().unwrap().to_string().clone();

        if mode != old_tm {
            info!("SET => Updating track mode");
            match self.send_command(Command::SetTrackingMode as i32, Some(mode_code.to_string())) {
                Ok(_) => {
                    info!("SET => Updated value track mode");
                    {
                        let mut tm = self.track_mode.write().unwrap();
                        tm.clear();
                        tm.push_str(&mode.to_owned());
                    }
                    return Ok(());
                }
                Err(e) => {
                    info!("SET => Not updated value track mode, COM error");
                    return Err(e);
                }
            }
        }
        info!("SET => Not updated track mode, same value");
        Ok(())
    }

    fn get_version(&mut self) -> String {
        let version = match self.send_command(Command::GetVersion as i32, None) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "Could not read the version from the hand controller: {:?}",
                    e
                );
                "000000".to_string()
            }
        };
        debug!("raw version: {}", version);
        let major = u8::from_str_radix(&version[0..2], 16).unwrap();
        let minor = u8::from_str_radix(&version[2..4], 16).unwrap();
        let patch = u8::from_str_radix(&version[4..6], 16).unwrap();
        format!("{}.{}.{}", major, minor, patch)
    }

    fn get_model(&mut self) -> String {
        let raw_model = match self.send_command(Command::GetModel as i32, None) {
            Ok(m) => {
                info!("Model: {:?}", &m.as_bytes());
                m
            }
            Err(e) => {
                error!("Could not read the mount model: {:?}", e);
                "UNKNOWN".to_string()
            }
        };

        match u8::from_str_radix(&raw_model[0..1], 16).unwrap() {
            0 => String::from("EQ6"),
            1 => String::from("HEQ5"),
            2 => String::from("EQ5"),
            3 => String::from("EQ3"),
            4 => String::from("EQ8"),
            5 => String::from("AZ-EQ6"),
            6 => String::from("AZ-EQ5"),
            128..=143 => String::from("AZ"),
            144..=159 => String::from("DOB"),
            _ => String::from("AllView"),
        }
    }

    fn is_aligned(&mut self) {
        let raw_value = match self.send_command(Command::GetAlignment as i32, None) {
            Ok(m) => {
                info!("Aligned: {:?}", &m);
                m
            }
            Err(e) => {
                error!("Could not read the mount alignment: {:?}", e);
                "UNKNOWN".to_string()
            }
        };

        let status = match &raw_value.chars().next() {
            Some(v) => match *v as i32 {
                1 => "true",
                _ => "false",
            },
            _ => {
                error!("Cannot read alignment value");
                "false"
            }
        };

        let aligned = self.aligned.read().unwrap().clone();

        if status != aligned.to_string() {
            {
                let mut a = self.aligned.write().unwrap();
                a.clear();
                a.push_str(&status);
            }
        }
    }

    fn init_props(&mut self) {
        let version = self.get_version();
        //self.name = self.get_model() + &self.name;
        self.is_aligned();
        // Build the version prop, always immutable
        self.static_properties.push(Property {
            name: String::from("SYNSCAN_VERSION"),
            kind: String::from("string"),
            value: version,
            permission: Permission::ReadOnly as i32,
        });

        self.properties.push(CustomProp {
            name: String::from("TRACKING_MODE"),
            kind: String::from("integer"),
            permission: Permission::ReadWrite,
            value: self.track_mode.clone(),
        });

        self.properties.push(CustomProp {
            name: String::from("ALIGNED"),
            kind: String::from("boolean"),
            permission: Permission::ReadOnly,
            value: self.aligned.clone(),
        })
    }
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

#[cfg(test)]
mod test {
    use astrotools::AstroSerialDevice;
    use crate::MountDevice;
    use env_logger::Env;

    #[test]
    fn test_new() {
	let env = Env::default().filter_or("LS_LOG_LEVEL", "info");
	env_logger::init_from_env(env);	
	let m = MountDevice::new("lol", "/abc", 9120, 1000);
    }
}
