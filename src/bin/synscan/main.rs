use astrotools::utils::build_server_address;
use astrotools::AstroSerialDevice;
use env_logger::Env;
use lightspeed_astro::devices::actions::DeviceActions;
use lightspeed_astro::devices::ProtoDevice;
use lightspeed_astro::props::{SetPropertyRequest, SetPropertyResponse};
use lightspeed_astro::request::GetDevicesRequest;
use lightspeed_astro::request::{CcdExposureRequest, CcdExposureResponse};
use lightspeed_astro::response::GetDevicesResponse;
use lightspeed_astro::server::astro_service_server::{AstroService, AstroServiceServer};
use log::{debug, error, info};
use tonic::{transport::Server, Request, Response, Status};

use std::sync::{Arc, RwLock};
use std::time::Duration;

mod synscan;
use synscan::{look_for_devices, MountDevice};

#[derive(Default, Clone)]
struct SynScanDriver {
    devices: Vec<Arc<RwLock<MountDevice>>>,
}

impl SynScanDriver {
    fn new() -> Self {
        let found = look_for_devices();
        let mut devices: Vec<Arc<RwLock<MountDevice>>> = Vec::new();
        for dev in found {
            let mut device_name = String::from("EQ6-r");
            debug!("name: {}", dev.0);
            debug!("info: {:?}", dev.1);

            if let Some(serial) = dev.1.serial_number {
                device_name = device_name + "-" + &serial
            }
            if let Some(device) = MountDevice::new(&device_name, &dev.0, 9600, 5000) {
                devices.push(Arc::new(RwLock::new(device)));
            } else {
                error!("Cannot start communication with {}", &device_name);
            }
        }
        Self { devices }
    }
}

#[tonic::async_trait]
impl AstroService for SynScanDriver {
    async fn expose(
        &self,
        request: Request<CcdExposureRequest>,
    ) -> Result<Response<CcdExposureResponse>, Status> {
        let reply = CcdExposureResponse { data: vec![] };
        Ok(Response::new(reply))
    }

    async fn get_devices(
        &self,
        request: Request<GetDevicesRequest>,
    ) -> Result<Response<GetDevicesResponse>, Status> {
        debug!(
            "Got a request to query devices from {:?}",
            request.remote_addr()
        );

        if self.devices.is_empty() {
            let reply = GetDevicesResponse { devices: vec![] };
            Ok(Response::new(reply))
        } else {
            let mut devices = Vec::new();
            for dev in self.devices.iter() {
                let device = dev.read().unwrap();
                let d = ProtoDevice {
                    id: device.get_id().to_string(),
                    name: device.get_name().to_owned(),
                    family: 0,
                    properties: device.properties.to_owned(),
                };
                devices.push(d);
            }
            let reply = GetDevicesResponse { devices };
            Ok(Response::new(reply))
        }
    }

    async fn set_property(
        &self,
        request: Request<SetPropertyRequest>,
    ) -> Result<Response<SetPropertyResponse>, Status> {
        info!(
            "Got a request to set a property from {:?}",
            request.remote_addr()
        );
        let message = request.get_ref();
        debug!("device_id: {:?}", message.device_id);

        if message.device_id == "" || message.property_name == "" || message.property_value == "" {
            return Ok(Response::new(SetPropertyResponse {
                status: DeviceActions::InvalidValue as i32,
            }));
        };

        // TODO: return case if no devices match
        for d in self.devices.iter() {
            let mut device = d.write().unwrap();
            if device.get_id().to_string() == message.device_id {
                info!(
                    "Updating property {} for {} to {}",
                    message.property_name, message.device_id, message.property_value,
                );

                if let Err(e) =
                    device.update_property(&message.property_name, &message.property_value)
                {
                    info!(
                        "Updating property {} for {} failed with reason: {:?}",
                        message.property_name, message.device_id, e
                    );
                    return Ok(Response::new(SetPropertyResponse { status: e as i32 }));
                }
            }
        }

        let reply = SetPropertyResponse {
            status: DeviceActions::Ok as i32,
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Default to log level INFO if LS_LOG_LEVEL is not set as
    // an env var
    let env = Env::default().filter_or("LS_LOG_LEVEL", "info");
    env_logger::init_from_env(env);

    // Reflection service
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(lightspeed_astro::proto::FD_DESCRIPTOR_SET)
        .build()
        .unwrap();

    let host = "127.0.0.1";
    let addr = build_server_address(host);
    let driver = SynScanDriver::new();

    let mut devices_for_fetching = Vec::new();
    let mut devices_for_closing = Vec::new();
    for d in &driver.devices {
        devices_for_fetching.push(Arc::clone(d));
        devices_for_closing.push(Arc::clone(d));
    }

    for d in &devices_for_fetching {
        let device = Arc::clone(d);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                device.write().unwrap().fetch_props();
            }
        });
    }

    info!("SynScan driver process listening on {}", addr);
    Server::builder()
        .add_service(reflection_service)
        .add_service(AstroServiceServer::new(driver))
        .serve(addr)
        .await?;
    Ok(())
}
