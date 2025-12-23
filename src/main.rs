use std::{ffi::CString, io::Write};

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
    /// Apply the commands to a specific light. Use `list` to get the paths.
    #[clap(long, value_name = "PATH")]
    path: Vec<String>,

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
        bail!("Expected format of [red,yellow,green,blue,white]:[on,off,blink] got {s}");
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

    // Parse out --all and --path entries into a list of Lights
    let hidapi = HidApi::new()?;
    let mut lights = vec![];
    if args.all {
        for device in Light::get_devices(&hidapi) {
            let light = Light::new(device.open_device(&hidapi)?);
            lights.push(light);
        }
    } else {
        for path in &args.path {
            let path_cstring = CString::new(path.as_str())?;
            let device = hidapi.open_path(&path_cstring)?;
            let light = Light::new(device);
            lights.push(light);
        }
    }

    if lights.is_empty() {
        bail!("No lights found");
    }

    // Calculate LightCommandSet
    let mut lightset = if args.reset {
        LightCommandSet::all_off()
    } else {
        LightCommandSet::default()
    };

    for (color, lightmode) in &args.commands {
        lightset.set(*color, *lightmode);
    }
    
    // Send command to the lights
    for light in &lights {
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

#[cfg(test)]
mod tests {
    use super::*;
    use qlight::{Color, LightMode};

    #[test]
    fn parse_command_ok_basic() {
        let cmd = parse_command("red:on").expect("should parse");
        assert_eq!(cmd, (Color::Red, LightMode::On));
    }

    #[test]
    fn parse_command_ok_case_insensitive() {
        let cmd = parse_command("GrEeN:BlInK").expect("should parse case-insensitively");
        assert_eq!(cmd, (Color::Green, LightMode::Blink));
    }

    #[test]
    fn parse_command_err_missing_colon() {
        let err = parse_command("red").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Expected format of"), "got: {msg}");
    }

    #[test]
    fn parse_command_err_bad_color() {
        let err = parse_command("purple:on").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Expected one of [red, yellow, green, blue, white]"), "got: {msg}");
    }

    #[test]
    fn parse_command_err_bad_mode() {
        let err = parse_command("red:florb").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Expected one of [on, off, blink]"), "got: {msg}");
    }

    #[test]
    fn set_args_mutually_exclusive_all_and_path() {
        // specifying both --all and --path should be rejected by clap as an argument conflict
        let err = Args::try_parse_from([
            "qlight",
            "set",
            "--all",
            "--path",
            "/dev/fake1",
            "red:on",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn set_args_multiple_paths_allowed() {
        // --path may be specified multiple times to target multiple devices
        let args = Args::try_parse_from([
            "qlight",
            "set",
            "--path",
            "/dev/fake1",
            "--path",
            "/dev/fake2",
            "red:on",
        ])
        .expect("should parse multiple --path entries");

        match args.action {
            Action::Set(set) => {
                assert_eq!(set.path, vec!["/dev/fake1".to_string(), "/dev/fake2".to_string()]);
                assert!(!set.all, "--all should not be set");
            }
            _ => panic!("expected set subcommand"),
        }
    }

    #[test]
    fn set_args_all_only_parses() {
        // --all alone should parse and set `all` to true with no paths
        let args = Args::try_parse_from([
            "qlight",
            "set",
            "--all",
            "red:on",
        ])
        .expect("should parse --all");

        match args.action {
            Action::Set(set) => {
                assert!(set.all, "--all should be set");
                assert!(set.path.is_empty(), "no paths should be present");
            }
            _ => panic!("expected set subcommand"),
        }
    }
}
