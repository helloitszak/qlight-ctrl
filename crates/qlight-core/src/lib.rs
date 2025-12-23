use hidapi::{DeviceInfo, HidApi, HidDevice, HidError};

const VID: u16 = 0x04d8;
const PID: u16 = 0xe73c;
const REPORT_ID: u8 = 0x57;

pub type LightCommand = (Color, LightMode);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)?;
        Ok(())
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Color {
    Red = 2,
    Yellow = 3,
    Green = 4,
    Blue = 5,
    White = 6,
}

impl TryFrom<&str> for Color {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let color = match value.to_lowercase().as_str() {
            "red" => Color::Red,
            "yellow" => Color::Yellow,
            "green" => Color::Green,
            "blue" => Color::Blue,
            "white" => Color::White,
            other => {
                return Err(ParseError(format!(
                    "Expected one of [red, yellow, green, blue, white], got {other}"
                )))
            }
        };

        Ok(color)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum LightMode {
    Off = 0,
    On = 1,
    Blink = 2,
    Ignore = 3,
}

impl TryFrom<&str> for LightMode {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let light_mode = match value.to_lowercase().as_str() {
            "on" => LightMode::On,
            "off" => LightMode::Off,
            "blink" => LightMode::Blink,
            other => {
                return Err(ParseError(format!(
                    "Expected one of [on, off, blink] in command, got {other}"
                )))
            }
        };

        Ok(light_mode)
    }
}

impl Default for LightMode {
    fn default() -> Self {
        Self::Ignore
    }
}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub enum SoundMode {
    Off = 0,
    Noise1 = 1,
    Noise2 = 2,
    Noise3 = 3,
    Noise4 = 4,
    Noise5 = 5,
    Ignore = 6,
}

impl Default for SoundMode {
    fn default() -> Self {
        Self::Ignore
    }
}

#[derive(Default, Debug)]
pub struct LightCommandSet {
    pub red: LightMode,
    pub yellow: LightMode,
    pub green: LightMode,
    pub blue: LightMode,
    pub white: LightMode,
    pub sound: SoundMode,
}

impl LightCommandSet {
    pub fn all_off() -> Self {
        Self {
            red: LightMode::Off,
            yellow: LightMode::Off,
            green: LightMode::Off,
            blue: LightMode::Off,
            white: LightMode::Off,
            sound: SoundMode::Off,
        }
    }

    pub fn set(&mut self, color: Color, light_mode: LightMode) {
        match color {
            Color::Red => self.red = light_mode,
            Color::Yellow => self.yellow = light_mode,
            Color::Green => self.green = light_mode,
            Color::Blue => self.blue = light_mode,
            Color::White => self.white = light_mode,
        }
    }

    fn to_report(&self) -> [u8; 65] {
        let mut data: [u8; 65] = [0x0; 65];
        data[0] = REPORT_ID;
        data[2] = self.red as u8;
        data[3] = self.yellow as u8;
        data[4] = self.green as u8;
        data[5] = self.blue as u8;
        data[6] = self.white as u8;
        data[7] = self.sound as u8;
        data
    }
}

pub struct Light {
    device: HidDevice,
}

impl Light {
    pub fn new(device: HidDevice) -> Self {
        // TODO: Should I check if this is the right type of device?
        Self { device }
    }

    pub fn get_devices(hidapi: &HidApi) -> impl Iterator<Item = &DeviceInfo> {
        hidapi
            .device_list()
            .filter(|x| x.vendor_id() == VID && x.product_id() == PID)
    }

    pub fn update(&self, light_set: &LightCommandSet) -> Result<usize, HidError> {
        self.device.write(&light_set.to_report())
    }
}
