use anyhow::{Context, Result};
use config::Config;
use hidapi::HidApi;
use matchit::{Match, Router};
use qlight_core::{Color, Light, LightCommandSet, LightMode};
use rosc::OscPacket;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::CString;
use std::net::{SocketAddrV4, UdpSocket};
use std::str::FromStr;
use tracing::{info, trace, warn};

const DEFAULT_CONFIG_PATH: &str = "config.toml";
const CONFIG_ENV_VAR: &str = "QLIGHT_OSC_CONFIG";

#[derive(Debug)]
struct LightThing {
    binding: DeviceBinding,
    light: Option<Light>
}

impl LightThing {
    fn new(binding: DeviceBinding) -> Self {
        Self {
            binding,
            light: Default::default()
        }
    }

    fn get_or_init_light(&mut self, hidapi: &HidApi) -> Result<&Light> {
        if self.light.is_none() {
            let path = &self.binding.path;

            let device = hidapi
                .open_path(&CString::from_str(path)?)
                .with_context(|| format!("Failed to open HID device at path: {path}"))?;

            self.light = Some(Light::new(device));
        }

        // At this point, we just put a light in if it doesn't exit.
        Ok(self.light.as_ref().unwrap())
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DeviceBinding {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct AppConfig {
    listen: String,
    bindings: Option<HashMap<String, DeviceBinding>>
}

impl AppConfig {
    fn load_default() -> Result<Self> {
        let config_path = std::env::var(CONFIG_ENV_VAR)
            .unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());
        
        let config = Config::builder()
            .add_source(config::File::with_name(&config_path).required(true))
            .build()
            .with_context(|| format!("Failed to load config from {config_path}"))?;

        config
            .try_deserialize::<AppConfig>()
            .with_context(|| "Failed to deserialize configuration")
    }
}

// #[derive(Debug)]
struct QlightOsc {
    // light: Light,
    hidapi: HidApi,
    light: LightThing,
    router: Router<Command>,
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Color,
    Reset,
}

impl QlightOsc {
    fn new(hidapi: HidApi, binding: DeviceBinding) -> Self {
        let mut router = Router::new();
        router
            .insert("/lights/{id}/{color}", Command::Color)
            .expect("Failed to compile route");

        router
            .insert("/reset/{id}", Command::Reset)
            .expect("Failed to compile route");

        QlightOsc { light: LightThing::new(binding), router, hidapi }
    }

    fn handle_packet(&mut self, packet: OscPacket) -> Result<()> {
        match packet {
            OscPacket::Message(msg) => {
                match self.router.at(&msg.addr) {
                    Ok(m @ Match {
                        value: Command::Color,
                        ..
                    }) => {
                        let id = m.params
                            .get("id")
                            .expect("Color command should always have an id");
                        let color_str = m.params
                            .get("color")
                            .expect("Color command should always have a color");
                

                        if let Some(lcs) = self.handle_color_command(&msg, id, color_str) {
                            match self.light.get_or_init_light(&self.hidapi) {
                                Ok(light) => { light.update(&lcs)?; },
                                Err(e) => warn!("Failed to update {:?}: {}", self.light, e)
                            }
                        }

                        Ok(())
                    }
                    Ok(m @ Match {
                        value: Command::Reset,
                        ..
                    }) => {
                        let id = m.params
                            .get("id")
                            .expect("Reset command should always have an id");
                        if let Some(lcs) = self.handle_reset_command(&msg, id) {
                            match self.light.get_or_init_light(&self.hidapi) {
                                Ok(light) => { light.update(&lcs)?; },
                                Err(e) => warn!("Failed to update {:?}: {}", self.light, e)
                            }
                        }
                        Ok(())
                    }
                    _ => {
                        warn!("Ignoring message for unknown OSC path: {}", &msg.addr);
                        Ok(())
                    }
                }
            }
            OscPacket::Bundle(_bundle) => {
                warn!("We don't support OSC Bundles... yet. Ignoring packet.");
                Ok(())
            }
        }
    }

    fn handle_color_command(&mut self, msg: &rosc::OscMessage, _id: &str, color_str: &str) -> Option<LightCommandSet> {

        let color = match color_str.to_lowercase().as_str() {
            "red" => Color::Red,
            "yellow" => Color::Yellow,
            "green" => Color::Green,
            "blue" => Color::Blue,
            "white" => Color::White,
            _ => {
                warn!("Ignoring message {} with unknown color {}", &msg.addr, color_str);
                return None;
            }
        };

        let lightmode = match msg.args.as_slice() {
            [rosc::OscType::Int(0)] => LightMode::Off,
            [rosc::OscType::Int(1)] => LightMode::On,
            [rosc::OscType::Int(2)] => LightMode::Blink,
            _ => {
                warn!("Ignoring message {} with unknown arguments {:?}", &msg.addr, msg.args);
                return None;
            }
        };

        let mut lcs: LightCommandSet = LightCommandSet::default();
        info!("Setting light {:?} to {:?}", color, lightmode);
        lcs.set(color, lightmode);

        Some(lcs)
    }

    fn handle_reset_command(&mut self, _msg: &rosc::OscMessage, _id: &str) -> Option<LightCommandSet> {
        let lcs: LightCommandSet = LightCommandSet::all_off();
        info!("Resetting light");
        Some(lcs)
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Load configuration from file (default: config.toml, or QLIGHT_OSC_CONFIG env var)
    let config = AppConfig::load_default()?;

    let addr = SocketAddrV4::from_str(&config.listen)
        .with_context(|| format!("Failed to parse listen address: {}", config.listen))?;

    let sock = UdpSocket::bind(addr).with_context(|| format!("Failed to bind to {addr}"))?;

    info!("Listening to {addr}");

    let mut buf = [0u8; rosc::decoder::MTU];

    let hidapi = HidApi::new()?;
    
    // Get the first device binding from config, or use the first detected device
    let device_path = if let Some(bindings) = &config.bindings {
        bindings
            .values()
            .next()
            .with_context(|| "No device bindings found in config")?
    } else {
        return Err(anyhow::anyhow!("No device bindings configured"));
    };


    let mut qlightosc = QlightOsc::new(hidapi, device_path.clone());

    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, addr)) => {
                trace!("Received packet with size {size} from: {addr}");
                let (_, packet) = rosc::decoder::decode_udp(&buf[..size])
                    .with_context(|| "Failed to read OSC packet".to_string())?;

                qlightosc.handle_packet(packet)?;
            }
            Err(e) => {
                trace!("Error receiving from socket: {e}");
                break;
            }
        }
    }

    Ok(())
}
