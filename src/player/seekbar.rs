use super::sdl::{Rect, Ui};

const DARK: (u8, u8, u8) = (16, 24, 32);
const LIGHT: (u8, u8, u8) = (255, 255, 255);
const ADVANCE: i32 = 12;
const HEIGHT: i32 = 28;

pub fn fraction(ui: &Ui, x: i32, y: i32) -> Option<f64> {
    let (w, h) = ui.size();
    (w >= 3 && h >= 3 && (0..w).contains(&x) && ((h - HEIGHT).max(0)..h).contains(&y))
        .then(|| x as f64 / (w - 1) as f64)
}

pub fn draw(ui: &Ui, position: i64, duration: Option<i64>) {
    let (w, h) = ui.size();
    if w < 3 || h < 3 {
        return;
    }
    let track = Rect {
        x: 0,
        y: (h - HEIGHT).max(0),
        w,
        h: HEIGHT.min(h),
    };
    let current = format_time(position.max(0));
    let total = duration.map_or_else(|| "-".into(), format_time);
    let hover = duration.and_then(|duration| {
        let (x, y) = ui.mouse()?;
        Some((x, (fraction(ui, x, y)? * duration as f64).round() as i64))
    });
    let y = track.y + (track.h - 14) / 2;
    if let Some((_, target)) = hover {
        text(
            ui,
            &format!(" -> {}", format_time(target)),
            8 + text_width(&current),
            y,
        );
    }
    text(ui, &current, 8, y);
    text(ui, &total, w - 8 - text_width(&total), y);

    ui.color(DARK);
    ui.outline(track);
    ui.color(LIGHT);
    ui.outline(Rect {
        x: 1,
        y: track.y + 1,
        w: w - 2,
        h: track.h - 2,
    });
    if let Some(duration) = duration {
        let x = (i128::from(position.clamp(0, duration)) * i128::from(w - 1) / i128::from(duration))
            as i32;
        marker(ui, x, track.y, h, false);
    }
    if let Some((x, _)) = hover {
        marker(ui, x, track.y, h, true);
    }
}

fn marker(ui: &Ui, x: i32, mut y: i32, bottom: i32, dashed: bool) {
    while y < bottom {
        let remaining = bottom - y;
        let h = if dashed && remaining > 8 {
            4
        } else {
            remaining
        };
        ui.color(DARK);
        ui.fill(Rect {
            x: x - 1,
            y,
            w: 3,
            h,
        });
        ui.color(LIGHT);
        ui.fill(Rect { x, y, w: 1, h });
        y += if dashed { 8 } else { h };
    }
}

fn format_time(us: i64) -> String {
    let tenths = us / 100_000;
    let hours = tenths / 36_000;
    let minutes = tenths / 600 % 60;
    let seconds = tenths / 10 % 60;
    let fraction = tenths % 10;
    if hours != 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}.{fraction}")
    } else if minutes != 0 {
        format!("{minutes:02}:{seconds:02}.{fraction}")
    } else {
        format!("{seconds:02}.{fraction}")
    }
}

fn glyph(ch: u8) -> [u8; 7] {
    const DIGITS: [[u8; 7]; 10] = [
        [14, 17, 19, 21, 25, 17, 14],
        [4, 12, 4, 4, 4, 4, 14],
        [14, 17, 1, 2, 4, 8, 31],
        [30, 1, 1, 14, 1, 1, 30],
        [2, 6, 10, 18, 31, 2, 2],
        [31, 16, 16, 30, 1, 1, 30],
        [14, 16, 16, 30, 17, 17, 14],
        [31, 1, 2, 4, 8, 8, 8],
        [14, 17, 17, 14, 17, 17, 14],
        [14, 17, 17, 15, 1, 1, 14],
    ];
    match ch {
        b'0'..=b'9' => DIGITS[(ch - b'0') as usize],
        b':' => [0, 0, 4, 0, 4, 0, 0],
        b'.' => [0, 0, 0, 0, 0, 0, 4],
        b'-' => [0, 0, 0, 14, 0, 0, 0],
        b'>' => [0, 8, 4, 2, 4, 8, 0],
        _ => [0; 7],
    }
}

fn text_width(text: &str) -> i32 {
    text.len() as i32 * ADVANCE
}

fn text(ui: &Ui, text: &str, x: i32, y: i32) {
    for (outline, color) in [(1, DARK), (0, LIGHT)] {
        ui.color(color);
        let mut pixels = Vec::with_capacity(text.len() * 35);
        for (i, ch) in text.bytes().enumerate() {
            for (row, bits) in glyph(ch).into_iter().enumerate() {
                for col in 0..5 {
                    if bits & (16 >> col) != 0 {
                        pixels.push(Rect {
                            x: x + i as i32 * ADVANCE + col * 2 - outline,
                            y: y + row as i32 * 2 - outline,
                            w: 2 + outline * 2,
                            h: 2 + outline * 2,
                        });
                    }
                }
            }
        }
        ui.fill_many(&pixels);
    }
}
