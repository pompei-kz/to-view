use sha2::{Digest, Sha256};
use std::{
  env,
  fs::{self, OpenOptions},
  io::{self, Write},
  os::unix::ffi::OsStrExt,
  path::{Path, PathBuf},
  process,
  time::{SystemTime, UNIX_EPOCH},
};

fn directory(path: &Path, application: &str) -> Option<PathBuf> {
  let home = PathBuf::from(env::var_os("HOME")?);
  let hash = format!("{:x}", Sha256::digest(path.as_os_str().as_bytes()));
  home.is_absolute().then(|| {
    home
      .join(".config")
      .join(application)
      .join("saves")
      .join(hash)
      .join("window")
  })
}

fn read_pair(path: &Path) -> Option<(i32, i32)> {
  let contents = fs::read_to_string(path).ok()?;
  let mut values = contents.split_whitespace();
  let first = values.next()?.parse().ok()?;
  let second = values.next()?.parse().ok()?;
  values.next().is_none().then_some((first, second))
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Geometry {
  pub position: Option<(i32, i32)>,
  pub size: Option<(i32, i32)>,
}

impl Geometry {
  pub fn load(path: &Path) -> Self {
    let Some(dir) = directory(path, "toview") else {
      return Self::default();
    };
    let legacy = directory(path, "to-view");
    let read = |name: &str| {
      let current = dir.join(name);
      if current.exists() {
        read_pair(&current)
      } else {
        legacy
          .as_ref()
          .and_then(|legacy| read_pair(&legacy.join(name)))
      }
    };
    Self {
      position: read("position.txt"),
      size: read("size.txt").filter(|&(w, h)| w > 0 && h > 0),
    }
  }

  pub fn save(self, path: &Path) {
    let Some(dir) = directory(path, "toview") else {
      return;
    };
    if let Err(error) = fs::create_dir_all(&dir) {
      eprintln!(
        "toview: {}",
        crate::i18n::format(
          "could not create {path}: {error}",
          &[
            ("path", &dir.display().to_string()),
            ("error", &error.to_string())
          ]
        )
      );
      return;
    }
    for (name, pair) in [("position.txt", self.position), ("size.txt", self.size)] {
      if let Some((first, second)) = pair {
        if let Err(error) = write_pair(&dir, name, first, second) {
          eprintln!(
            "toview: {}",
            crate::i18n::format(
              "could not save {path}: {error}",
              &[
                ("path", &dir.join(name).display().to_string()),
                ("error", &error.to_string())
              ]
            )
          );
        }
      }
    }
  }
}

fn write_pair(dir: &Path, name: &str, first: i32, second: i32) -> io::Result<()> {
  let time = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default();
  let temporary = dir.join(format!(".{name}.{}-{}.tmp", process::id(), time.as_nanos()));
  let mut file = OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(&temporary)?;
  let result = writeln!(file, "{first} {second}");
  drop(file);
  let result = result.and_then(|()| fs::rename(&temporary, dir.join(name)));
  if result.is_err() {
    let _ = fs::remove_file(temporary);
  }
  result
}
