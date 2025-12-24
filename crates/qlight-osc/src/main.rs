use anyhow::{Context, Result};
use hidapi::HidApi;
use matchit::{Match, Router};
use qlight_core::{Color, Light, LightCommandSet, LightMode};
use rosc::OscPacket;
use std::ffi::CString;
use std::net::{SocketAddrV4, UdpSocket};
use std::str::FromStr;
use tracing::{info, trace, warn};

#[derive(Debug)]
struct QlightOsc {
    light: Light,
    router: Router<Command>,
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Color,
    Reset,
}

impl QlightOsc {
    fn new(light: Light) -> Self {
        let mut router = Router::new();
        router
            .insert("/lights/{id}/{color}", Command::Color)
            .expect("Failed to compile route");

        router
            .insert("/reset/{id}", Command::Reset)
            .expect("Failed to compile route");

        QlightOsc { light, router }
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
                            self.light.update(&lcs)?;
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
                            self.light.update(&lcs)?;
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

    let addr = SocketAddrV4::from_str("127.0.0.1:8000")
        .with_context(|| "Failed to parse IP".to_string())?;
    let sock = UdpSocket::bind(addr).with_context(|| "Failed to bind to ip".to_string())?;
    info!("Listening to {addr}");

    let mut buf = [0u8; rosc::decoder::MTU];

    let hidapi = HidApi::new().unwrap();
    let device = hidapi
        .open_path(&CString::from_str("DevSrvsID:4301069978")?)
        .with_context(|| "Failed to open HID Device".to_string())?;

    let mut qlightosc = QlightOsc::new(Light::new(device));

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
