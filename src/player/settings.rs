use std::{
  borrow::Cow,
  fs,
  io::{self, Write},
  path::Path,
  time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct PictureSettings {
  pub brightness: i32,
  pub contrast: i32,
  pub gamma: i32,
  pub saturation: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settings {
  pub picture: PictureSettings,
  pub volume: i32,
  pub muted: bool,
}

impl Default for Settings {
  fn default() -> Self {
    Self {
      picture: PictureSettings::default(),
      volume: 1000,
      muted: false,
    }
  }
}

// Volume is stored in tenths of a percent, so each 2.5% step is exact.
fn parse_volume(value: &str) -> Option<i32> {
  let (whole, fraction) = value.split_once('.').unwrap_or((value, "0"));
  if fraction.len() != 1 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
    return None;
  }
  let whole: i32 = whole.parse().ok()?;
  let value = whole.checked_mul(10)?.checked_add(fraction.parse().ok()?)?;
  (0..=1000).contains(&value).then_some(value)
}

impl Settings {
  pub fn volume_text(self) -> String {
    if self.volume % 10 == 0 {
      (self.volume / 10).to_string()
    } else {
      format!("{}.{}", self.volume / 10, self.volume % 10)
    }
  }

  pub fn load(path: &Path) -> Self {
    let mut result = Self::default();
    let Some(dir) = crate::window_geometry::directory(path, "toview") else {
      return result;
    };
    let Ok(text) = fs::read_to_string(dir.join("playback/settings.txt")) else {
      return result;
    };
    for line in text.lines() {
      let Some((key, value)) = line.split_once('=') else {
        continue;
      };
      let value = value.trim();
      let number = value.parse::<i32>().ok();
      match key.trim() {
        "brightness" => {
          if let Some(n) = number.filter(|n| (-100..=100).contains(n)) {
            result.picture.brightness = n
          }
        }
        "contrast" => {
          if let Some(n) = number.filter(|n| (-100..=100).contains(n)) {
            result.picture.contrast = n
          }
        }
        "gamma" => {
          if let Some(n) = number.filter(|n| (-100..=100).contains(n)) {
            result.picture.gamma = n
          }
        }
        "saturation" => {
          if let Some(n) = number.filter(|n| (-100..=100).contains(n)) {
            result.picture.saturation = n
          }
        }
        "volume" => {
          if let Some(n) = parse_volume(value) {
            result.volume = n
          }
        }
        "muted" => match value {
          "true" => result.muted = true,
          "false" => result.muted = false,
          _ => {}
        },
        _ => {}
      }
    }
    result
  }

  pub fn save(self, path: &Path) -> io::Result<()> {
    let dir = crate::window_geometry::directory(path, "toview")
      .ok_or_else(|| io::Error::other("HOME is not an absolute path"))?
      .join("playback");
    fs::create_dir_all(&dir)?;
    let time = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap_or_default()
      .as_nanos();
    let temp = dir.join(format!(".settings.{}-{time}.tmp", std::process::id()));
    let result = (|| {
      let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
      writeln!(
        file,
        "brightness={}\ncontrast={}\ngamma={}\nsaturation={}\nvolume={}\nmuted={}",
        self.picture.brightness,
        self.picture.contrast,
        self.picture.gamma,
        self.picture.saturation,
        self.volume_text(),
        self.muted
      )?;
      drop(file);
      fs::rename(&temp, dir.join("settings.txt"))
    })();
    if result.is_err() {
      let _ = fs::remove_file(&temp);
    }
    result
  }
}

impl PictureSettings {
  pub fn apply(self, pixels: &[u8]) -> Cow<'_, [u8]> {
    if self == Self::default() {
      return Cow::Borrowed(pixels);
    }
    let brightness = self.brightness as f32 / 100.0;
    let contrast = 2_f32.powf(self.contrast as f32 / 100.0);
    let gamma = 2_f32.powf(-self.gamma as f32 / 100.0);
    let saturation = 2_f32.powf(self.saturation as f32 / 100.0);
    let mut lut = [0_f32; 256];
    for (i, value) in lut.iter_mut().enumerate() {
      *value =
        (((i as f32 / 255.0 - 0.5) * contrast + 0.5 + brightness).clamp(0.0, 1.0)).powf(gamma);
    }
    let mut result = pixels.to_vec();
    for pixel in result.chunks_exact_mut(4) {
      let r = lut[pixel[0] as usize];
      let g = lut[pixel[1] as usize];
      let b = lut[pixel[2] as usize];
      let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
      for (channel, value) in pixel[..3].iter_mut().zip([r, g, b]) {
        *channel = ((y + (value - y) * saturation).clamp(0.0, 1.0) * 255.0).round() as u8;
      }
    }
    Cow::Owned(result)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn volume_uses_exact_tenths_and_reads_old_values() {
    assert_eq!(parse_volume("100"), Some(1000));
    assert_eq!(parse_volume("97.5"), Some(975));
    assert_eq!(parse_volume("100.1"), None);
    assert_eq!(
      Settings {
        volume: 975,
        ..Settings::default()
      }
      .volume_text(),
      "97.5"
    );
  }

  #[test]
  fn neutral_and_modified_pixels() {
    let source = [0, 127, 255, 42, 255, 0, 128, 255];
    assert_eq!(PictureSettings::default().apply(&source).as_ref(), source);
    let result = PictureSettings {
      brightness: 20,
      ..Default::default()
    }
    .apply(&source);
    assert!(result[0] > source[0]);
    assert_eq!(result[3], 42);
  }
}
