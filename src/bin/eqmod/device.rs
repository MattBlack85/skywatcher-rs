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
use std::fmt::UpperHex;
use std::io::{Read, Write};
use std::time::Duration;
use uuid::Uuid;

const SIDEREAL_RATE: f64 = 2.0 * 3.14 / 86164.09065;

enum RaCommand {
    Init = 0x3a4631,
    MotorBoardVersion = 0x3a6531,
    InquireGridPerRevolution = 0x3a6131,
    GetAxisPosition = 0x3a6a31,
    SetAxisPosition = 0x3a4531,
    GetAxisStatus = 0x3a6631,
}

enum DecCommand {
    Init = 0x3a4632,
    InquireGridPerRevolution = 0x3a6132,
    GetAxisPosition = 0x3a6a32,
    SetAxisPosition = 0x3a4531,
    GetAxisStatus = 0x3a6632,
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

            if let Err(_) = dev.send_command(DecCommand::Init as i32, None) {
                debug!("{}", DeviceActions::CannotConnect as i32);
                return None;
            }

            if let Err(_) = dev.send_command(RaCommand::Init as i32, None) {
                debug!("{}", DeviceActions::CannotConnect as i32);
                return None;
            }

            dev.init_device();
            dev.fetch_props();
            Some(dev)
        } else {
            debug!("{}", DeviceActions::CannotConnect as i32);
            None
        }
    }

    fn fetch_props(&mut self) {
        info!("Fetching props");

        let axis_pos = self.get_axis_position();
        println!("{}:{}", axis_pos.0, axis_pos.1);

        let axis_status = self.get_axis_status();
        println!("{}:{}", axis_status.0, axis_status.1);
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

        // Cast the hex string to a sequence of bytes
        let mut command: Vec<u8> = Vec::from_hex(hex_command).expect("Invalid Hex String");

        // append 13 at the end
        command.push(0x0d);
        debug!("COMMAND: {:?}", command);

        match self.port.write(&command) {
            Ok(_) => {
                debug!(
                    "Sent command: {}",
                    std::str::from_utf8(&command[..command.len() - 1]).unwrap()
                );
                let mut final_buf: Vec<u8> = Vec::new();
                debug!("Receiving data");

                loop {
                    let mut read_buf = [0; 1];

                    match self.port.read(read_buf.as_mut_slice()) {
                        Ok(_) => {
                            let byte = read_buf[0];
                            //debug!("Read byte: {}", byte);
                            final_buf.push(byte);

                            if byte == 0x0d as u8 {
                                break;
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                            error!("Timeout");
                            return Err(DeviceActions::Timeout);
                        }
                        Err(e) => error!("{:?}", e),
                    }
                }

                // Use this to check if the response is OK (=) or there is an error (!)
                if final_buf[0] == 0x3d {
                    let response =
                        std::str::from_utf8(&final_buf[1..&final_buf.len() - 1]).unwrap();
                    info!("RESPONSE: {}", response);
                    Ok(response.to_owned())
                } else {
                    Err(DeviceActions::InvalidValue)
                }
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

trait EQModMount {
    fn init_device(&mut self);
    fn get_motor_board_version(&mut self) -> u32;
    fn get_grid_per_revolution(&mut self) -> (String, String);
    fn get_axis_position(&mut self) -> (String, String);
    fn set_ra_axis_position(&mut self, val: &str);
    fn set_dec_axis_position(&mut self, val: &str);
    fn get_axis_status(&mut self) -> (String, String);
}

impl EQModMount for MountDevice {
    fn init_device(&mut self) {
        self.get_motor_board_version();
        self.get_grid_per_revolution();
    }

    /// Returns the motor board version.
    fn get_motor_board_version(&mut self) -> u32 {
        let version = match self.send_command(RaCommand::MotorBoardVersion as i32, None) {
            Ok(v) => str_24bits_to_u32(v),
            Err(_) => 0x0,
        };
        version
    }

    /// Returns (RA grid, DEC grid) grids per revolution.
    fn get_grid_per_revolution(&mut self) -> (String, String) {
        let ra_grid = match self.send_command(RaCommand::InquireGridPerRevolution as i32, None) {
            Ok(v) => v,
            Err(_) => String::from("UNKNOWN"),
        };

        let dec_grid = match self.send_command(DecCommand::InquireGridPerRevolution as i32, None) {
            Ok(v) => v,
            Err(_) => String::from("UNKNOWN"),
        };

        (ra_grid, dec_grid)
    }

    fn get_axis_position(&mut self) -> (String, String) {
        let ra_pos = match self.send_command(RaCommand::GetAxisPosition as i32, None) {
            Ok(v) => v,
            Err(_) => String::from("UNKNOWN"),
        };

        let dec_pos = match self.send_command(DecCommand::GetAxisPosition as i32, None) {
            Ok(v) => v,
            Err(_) => String::from("UNKNOWN"),
        };

        (ra_pos, dec_pos)
    }

    fn set_ra_axis_position(&mut self, val: &str) {
        let ra_pos =
            match self.send_command(RaCommand::SetAxisPosition as i32, Some(val.to_string())) {
                Ok(v) => info!("Set RA Axis position to {}", v),
                Err(e) => error!("Error while setting RA position: {}", e as i32),
            };
    }

    fn set_dec_axis_position(&mut self, val: &str) {
        let ra_pos =
            match self.send_command(DecCommand::SetAxisPosition as i32, Some(val.to_string())) {
                Ok(v) => info!("Set DEC Axis position to {}", v),
                Err(e) => error!("Error while setting DEC position: {}", e as i32),
            };
    }

    fn get_axis_status(&mut self) -> (String, String) {
        let ra_status = match self.send_command(RaCommand::GetAxisStatus as i32, None) {
            Ok(v) => v,
            Err(_) => String::from("UNKNOWN"),
        };

        let dec_status = match self.send_command(DecCommand::GetAxisStatus as i32, None) {
            Ok(v) => v,
            Err(_) => String::from("UNKNOWN"),
        };

        (ra_status, dec_status)
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
