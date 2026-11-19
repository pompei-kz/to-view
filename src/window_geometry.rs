use std::{
  env,
  fs::{self, OpenOptions},
  io::{self, Write},
  path::{Path, PathBuf},
  process,
  time::{SystemTime, UNIX_EPOCH},
};

fn directory() -> Option<PathBuf> {
  let home = PathBuf::from(env::var_os("HOME")?);
  home
    .is_absolute()
    .then(|| home.join(".config/to-view/saves/window"))
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
  pub fn load() -> Self {
    let Some(dir) = directory() else {
      return Self::default();
    };
    Self {
      position: read_pair(&dir.join("position.txt")),
      size: read_pair(&dir.join("size.txt")).filter(|&(w, h)| w > 0 && h > 0),
    }
  }

  pub fn save(self) {
    let Some(dir) = directory() else { return };
    if let Err(error) = fs::create_dir_all(&dir) {
      eprintln!("to-view: could not create {}: {error}", dir.display());
      return;
    }
    for (name, pair) in [("position.txt", self.position), ("size.txt", self.size)] {
      if let Some((first, second)) = pair {
        if let Err(error) = write_pair(&dir, name, first, second) {
          eprintln!(
            "to-view: could not save {}: {error}",
            dir.join(name).display()
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
