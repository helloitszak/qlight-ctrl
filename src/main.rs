use std::io::Write;

use clap::{ArgGroup, Parser};
use hidapi::HidApi;
use qlight::{Color, Light, LightCommand, LightMode, LightCommandSet};

use anyhow::{bail, Result};

mod qlight;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(clap::Subcommand, Debug)]
enum Action {
    Set(SetArgs),
    /// List all lights connected to this system
    List,
}

/// Set the light to a specific set of colors
#[derive(Parser, Debug)]
#[clap(group(
    ArgGroup::new("picker")
        .required(true)
        .args(&["all", "path"])
))]
struct SetArgs {
    /// Apply the commands to a specific lights. Use `list` to get the paths.
    #[clap(long, value_name = "PATH")]
    path: Option<String>,

    /// Apply the commands to all detected lights.
    #[clap(long)]
    all: bool,

    /// If set, any unspecified color will be turned off.
    #[clap(long)]
    reset: bool,

    /// A list of [color]:[state]
    /// 
    /// Valid colors: red, yellow, green, blue, white
    ///
    /// Valid states: off, on, blink
    #[arg(value_parser = parse_command)]
    commands: Vec<LightCommand>,
}

fn parse_command(s: &str) -> Result<LightCommand> {
    let Some((color, mode_name)) = s.split_once(':') else {
        bail!("Expected format of [red,yellow,green,blue,white]:[on,off,blink] got {}", s);
    };

    let color = Color::try_from(color)?; 
    let light_mode = LightMode::try_from(mode_name)?;

    Ok((color, light_mode))
}

fn list(_args: Args) -> Result<()> {
    let hidapi = HidApi::new()?;
    let devices = Light::get_devices(&hidapi);

    let mut stdout = std::io::stdout().lock();

    for device in devices {
        stdout.write_all(device.path().to_bytes())?;
        writeln!(stdout)?;
    }
    Ok(())
}

fn set(args: SetArgs) -> Result<()> {
    let mut lightset = if args.reset {
        LightCommandSet::default_off()
    } else {
        LightCommandSet::default()
    };

    for (color, lightmode) in &args.commands {
        lightset.set(*color, *lightmode);
    }

    let hidapi = HidApi::new()?;
    for light in Light::get_devices(&hidapi) {
        let light = Light::new(light.open_device(&hidapi)?);
        light.update(&lightset)?;
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Args::parse();
    match cli.action {
        Action::Set(s) => set(s),
        Action::List => list(cli),
    }
}
