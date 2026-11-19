mod player;

use std::{env, ffi::OsString, path::PathBuf, process::ExitCode};

const USAGE: &str = "Usage: to-view [OPTIONS] [--] <video-file>

  --close-on-end=1, -j, -j1  Close the window at the end of playback
  --close-on-end=0, -J, -j0  Keep the window open (default)
  -h, --help                Show help
  -V, --version             Show version";

enum Command {
    Play(player::Options),
    Help,
    Version,
}

impl Command {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Self, String> {
        let mut path = None;
        let mut close_on_end = false;
        let mut parse_options = true;
        for arg in args {
            if parse_options {
                match arg.to_str() {
                    Some("-h" | "--help") => return Ok(Self::Help),
                    Some("-V" | "--version") => return Ok(Self::Version),
                    Some("--") => {
                        parse_options = false;
                        continue;
                    }
                    Some("--close-on-end=1" | "-j" | "-j1") => {
                        close_on_end = true;
                        continue;
                    }
                    Some("--close-on-end=0" | "-J" | "-j0") => {
                        close_on_end = false;
                        continue;
                    }
                    Some(option)
                        if option == "--close-on-end" || option.starts_with("--close-on-end=") =>
                    {
                        return Err("--close-on-end requires =0 or =1".into());
                    }
                    _ if arg.as_encoded_bytes().starts_with(b"-") => {
                        return Err(format!("unknown option {}", arg.to_string_lossy()));
                    }
                    _ => {}
                }
            }
            if path.is_some() {
                return Err("unexpected extra argument".into());
            }
            path = Some(PathBuf::from(arg));
        }
        Ok(Self::Play(player::Options {
            path: path.ok_or("missing video file")?,
            close_on_end,
        }))
    }
}

fn main() -> ExitCode {
    match Command::parse(env::args_os().skip(1)) {
        Ok(Command::Play(options)) => match player::play(options) {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("to-view: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("to-view {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("to-view: {error}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
