mod i18n;
mod player;
mod window_geometry;

use std::{
  env,
  ffi::OsString,
  fs::OpenOptions,
  os::fd::AsRawFd,
  path::{Path, PathBuf},
  process::ExitCode,
};

fn usage() -> String {
  format!(
    "{}\n\n  --close-on-end=1|on|true, -j   {}\n  --close-on-end=0|off|false, -J  {}\n  --out-err=PATH                  {}\n  -h, --help                      {}\n  -V, --version                   {}",
    i18n::tr("Usage: toview [OPTIONS] [--] <video-file>"),
    i18n::tr("Close the window at the end of playback"),
    i18n::tr("Keep the window open (default)"),
    i18n::tr("Redirect error output to a file"),
    i18n::tr("Show help"),
    i18n::tr("Show version")
  )
}

enum Command {
  Play {
    options: player::Options,
    error_output: Option<PathBuf>,
  },
  Help,
  Version,
}

impl Command {
  fn parse(args: impl Iterator<Item = OsString>) -> Result<Self, String> {
    let mut path = None;
    let mut error_output = None;
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
          Some("--close-on-end=1" | "--close-on-end=on" | "--close-on-end=true" | "-j") => {
            close_on_end = true;
            continue;
          }
          Some("--close-on-end=0" | "--close-on-end=off" | "--close-on-end=false" | "-J") => {
            close_on_end = false;
            continue;
          }
          Some(option) if option == "--close-on-end" || option.starts_with("--close-on-end=") => {
            return Err(i18n::tr(
              "--close-on-end requires =1, =on, =true, =0, =off, or =false",
            ));
          }
          Some(option) if option.starts_with("--out-err=") => {
            let value = option.split_once('=').unwrap().1;
            if value.is_empty() {
              return Err(i18n::tr("--out-err requires a non-empty path"));
            }
            error_output = Some(PathBuf::from(value));
            continue;
          }
          Some("--out-err") => {
            return Err(i18n::tr("--out-err requires =PATH"));
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
    Ok(Self::Play {
      options: player::Options {
        path: path.ok_or_else(|| i18n::tr("missing video file"))?,
        close_on_end,
      },
      error_output,
    })
  }
}

fn redirect_stderr(path: &Path) -> Result<(), String> {
  let file = OpenOptions::new()
    .create(true)
    .truncate(true)
    .write(true)
    .open(path)
    .map_err(|error| format!("{}: {error}", path.display()))?;
  if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
    return Err(std::io::Error::last_os_error().to_string());
  }
  Ok(())
}

fn main() -> ExitCode {
  i18n::init();
  match Command::parse(env::args_os().skip(1)) {
    Ok(Command::Play {
      options,
      error_output,
    }) => {
      if let Some(path) = error_output
        && let Err(error) = redirect_stderr(&path)
      {
        eprintln!("toview: {error}");
        return ExitCode::FAILURE;
      }
      match player::play(options) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
          eprintln!("toview: {error}");
          ExitCode::FAILURE
        }
      }
    }
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
