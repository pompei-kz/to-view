#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
  pub zoom: f64,
  pub offset_x: f64,
  pub offset_y: f64,
}

impl Default for ViewTransform {
  fn default() -> Self {
    Self {
      zoom: 1.0,
      offset_x: 0.0,
      offset_y: 0.0,
    }
  }
}

// The dimensions are the bounding box after rotation. The texture itself is
// swapped back for SDL_RenderCopyEx, which rotates around its centre.
pub fn fit(size: (i32, i32), aspect: f64, rotation: f64) -> Option<(f64, f64, bool)> {
  let (width, height) = (f64::from(size.0), f64::from(size.1));
  if width <= 0.0 || height <= 0.0 || !aspect.is_finite() || aspect <= 0.0 || !rotation.is_finite()
  {
    return None;
  }
  let sideways =
    rotation.abs().rem_euclid(180.0) > 45.0 && rotation.abs().rem_euclid(180.0) < 135.0;
  let aspect = if sideways { 1.0 / aspect } else { aspect };
  Some((
    width.min(height * aspect),
    height.min(width / aspect),
    sideways,
  ))
}

impl ViewTransform {
  pub fn center(self, size: (i32, i32), fit: (f64, f64)) -> (f64, f64) {
    (
      f64::from(size.0) / 2.0 + self.offset_x * fit.0,
      f64::from(size.1) / 2.0 + self.offset_y * fit.1,
    )
  }
  fn set_center(&mut self, size: (i32, i32), fit: (f64, f64), center: (f64, f64)) {
    self.offset_x = ((center.0 - f64::from(size.0) / 2.0) / fit.0).clamp(-1_000_000.0, 1_000_000.0);
    self.offset_y = ((center.1 - f64::from(size.1) / 2.0) / fit.1).clamp(-1_000_000.0, 1_000_000.0);
  }
  pub fn zoom_at(&mut self, size: (i32, i32), fit: (f64, f64), cursor: (i32, i32), delta: f64) {
    if !delta.is_finite() || fit.0 <= 0.0 || fit.1 <= 0.0 {
      return;
    }
    let next = (self.zoom * 1.1_f64.powf(delta)).clamp(0.1, 16.0);
    if next == self.zoom {
      return;
    }
    let center = self.center(size, fit);
    let ratio = next / self.zoom;
    let cursor = (f64::from(cursor.0), f64::from(cursor.1));
    self.set_center(
      size,
      fit,
      (
        cursor.0 + (center.0 - cursor.0) * ratio,
        cursor.1 + (center.1 - cursor.1) * ratio,
      ),
    );
    self.zoom = next;
  }
  pub fn pan(&mut self, size: (i32, i32), fit: (f64, f64), delta: (i32, i32)) {
    if fit.0 <= 0.0 || fit.1 <= 0.0 {
      return;
    }
    let center = self.center(size, fit);
    self.set_center(
      size,
      fit,
      (center.0 + f64::from(delta.0), center.1 + f64::from(delta.1)),
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn cursor_anchor_round_trip_and_limits() {
    let size = (1000, 600);
    for rotation in [0.0, 90.0, 180.0, 270.0] {
      let (w, h, _) = fit(size, 16.0 / 9.0, rotation).unwrap();
      for cursor in [(500, 300), (0, 0), (999, 599), (-100, 700)] {
        let mut view = ViewTransform::default();
        let before = view.center(size, (w, h));
        view.zoom_at(size, (w, h), cursor, 0.5);
        let after = view.center(size, (w, h));
        let ratio = view.zoom;
        assert!(
          (after.0 - (f64::from(cursor.0) + (before.0 - f64::from(cursor.0)) * ratio)).abs() < 1e-9
        );
        assert!(
          (after.1 - (f64::from(cursor.1) + (before.1 - f64::from(cursor.1)) * ratio)).abs() < 1e-9
        );
        view.zoom_at(size, (w, h), cursor, -0.5);
        assert!(
          (view.zoom - 1.0).abs() < 1e-12
            && view.offset_x.abs() < 1e-12
            && view.offset_y.abs() < 1e-12
        );
        view.zoom_at(size, (w, h), cursor, 10000.0);
        assert_eq!(view.zoom, 16.0);
        let old = view;
        view.zoom_at(size, (w, h), cursor, 1.0);
        assert_eq!(view, old);
        view.zoom_at(size, (w, h), cursor, -10000.0);
        assert_eq!(view.zoom, 0.1);
      }
    }
  }
  #[test]
  fn repeated_fractional_zoom_reverses_without_drift() {
    let size = (1920, 1080);
    let (w, h, _) = fit(size, 4.0 / 3.0, 90.0).unwrap();
    let mut view = ViewTransform::default();
    for _ in 0..1000 {
      view.zoom_at(size, (w, h), (83, 707), 0.01);
    }
    for _ in 0..1000 {
      view.zoom_at(size, (w, h), (83, 707), -0.01);
    }
    assert!((view.zoom - 1.0).abs() < 1e-10);
    assert!(view.offset_x.abs() < 1e-10 && view.offset_y.abs() < 1e-10);
  }

  #[test]
  fn pan_tracks_pixels_and_survives_resize() {
    let mut v = ViewTransform::default();
    let size = (800, 600);
    let (w, h, _) = fit(size, 16.0 / 9.0, 0.0).unwrap();
    v.pan(size, (w, h), (40, -30));
    assert_eq!(v.center(size, (w, h)), (440.0, 270.0));
    let size = (1600, 1200);
    let (w, h, _) = fit(size, 16.0 / 9.0, 0.0).unwrap();
    assert_eq!(v.center(size, (w, h)), (880.0, 540.0));
  }
}
