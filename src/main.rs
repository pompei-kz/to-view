mod player;

use std::{env, ffi::OsString, path::PathBuf, process::ExitCode};

const USAGE: &str = "Usage: to-view [--] <video-file>";

enum Command {
    Play(player::Options),
    Help,
    Version,
}

impl Command {
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Self, String> {
        let first = args.next().ok_or("missing video file")?;
        let command = match first.to_str() {
            Some("-h" | "--help") => Self::Help,
            Some("-V" | "--version") => Self::Version,
            Some("--") => Self::Play(player::Options {
                path: args.next().ok_or("missing video file after --")?.into(),
            }),
            _ if first.as_encoded_bytes().starts_with(b"-") => {
                return Err(format!("unknown option {}", first.to_string_lossy()));
            }
            _ => Self::Play(player::Options {
                path: PathBuf::from(first),
            }),
        };
        if args.next().is_some() {
            return Err("unexpected extra argument".into());
        }
        Ok(command)
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
