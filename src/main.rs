mod i18n;
mod player;
mod window_geometry;

use std::{env, ffi::OsString, path::PathBuf, process::ExitCode};

fn usage() -> String {
  format!(
    "{}\n\n  --close-on-end=1, -j, -j1  {}\n  --close-on-end=0, -J, -j0  {}\n  -h, --help                {}\n  -V, --version             {}",
    i18n::tr("Usage: toview [OPTIONS] [--] <video-file>"),
    i18n::tr("Close the window at the end of playback"),
    i18n::tr("Keep the window open (default)"),
    i18n::tr("Show help"),
    i18n::tr("Show version")
  )
}

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
          Some(option) if option == "--close-on-end" || option.starts_with("--close-on-end=") => {
            return Err(i18n::tr("--close-on-end requires =0 or =1"));
          }
          _ if arg.as_encoded_bytes().starts_with(b"-") => {
            return Err(i18n::format(
              "unknown option {option}",
              &[("option", &arg.to_string_lossy())],
            ));
          }
          _ => {}
        }
      }
      if path.is_some() {
        return Err(i18n::tr("unexpected extra argument"));
      }
      path = Some(PathBuf::from(arg));
    }
    Ok(Self::Play(player::Options {
      path: path.ok_or_else(|| i18n::tr("missing video file"))?,
      close_on_end,
    }))
  }
}

fn main() -> ExitCode {
  i18n::init();
  match Command::parse(env::args_os().skip(1)) {
    Ok(Command::Play(options)) => match player::play(options) {
      Ok(code) => ExitCode::from(code),
      Err(error) => {
        eprintln!("toview: {error}");
        ExitCode::FAILURE
      }
    },
    Ok(Command::Help) => {
      println!("{}", usage());
      ExitCode::SUCCESS
    }
    Ok(Command::Version) => {
      println!("toview {}", env!("CARGO_PKG_VERSION"));
      ExitCode::SUCCESS
    }
    Err(error) => {
      eprintln!("toview: {error}\n{}", usage());
      ExitCode::from(2)
    }
  }
}
