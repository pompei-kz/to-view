use super::sdl::{Rect, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Video,
    Wave,
    Spectrum,
}

impl Mode {
    pub fn next(self, has_video: bool) -> Self {
        match self {
            Self::Video => Self::Wave,
            Self::Wave => Self::Spectrum,
            Self::Spectrum if has_video => Self::Video,
            Self::Spectrum => Self::Wave,
        }
    }
}

pub fn draw(ui: &Ui, mode: Mode, samples: &[f32]) {
    let (width, height) = ui.size();
    if width < 2 || height < 2 || samples.is_empty() {
        return;
    }
    match mode {
        Mode::Video => {}
        Mode::Wave => {
            ui.color((255, 255, 255));
            let mut previous = height / 2;
            for x in 0..width {
                let sample = samples[x as usize * samples.len() / width as usize];
                let y = height / 2 - (sample.clamp(-1.0, 1.0) * (height / 2) as f32) as i32;
                ui.line(x.saturating_sub(1), previous, x, y);
                previous = y;
            }
        }
        Mode::Spectrum => {
            ui.color((86, 200, 220));
            const BANDS: usize = 40;
            let count = samples.len() as f32;
            let stride = (width / BANDS as i32).max(1);
            for bin in 1..=BANDS.min(width as usize) {
                let mut real = 0.0;
                let mut imaginary = 0.0;
                for (i, sample) in samples.iter().enumerate() {
                    let phase = std::f32::consts::TAU * i as f32 / count;
                    let windowed = sample * (0.5 - 0.5 * phase.cos());
                    let (sin, cos) = (bin as f32 * phase).sin_cos();
                    real += windowed * cos;
                    imaginary += windowed * sin;
                }
                let power = (real.hypot(imaginary) * 8.0 / count).clamp(0.0, 1.0);
                let h = (power.sqrt() * height as f32) as i32;
                ui.fill(Rect {
                    x: (bin as i32 - 1) * stride,
                    y: height - h,
                    w: (stride - 1).max(1),
                    h,
                });
            }
        }
    }
}
