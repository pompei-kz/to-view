use std::{
    env,
    ffi::c_int,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

fn directory() -> Option<PathBuf> {
    let home = PathBuf::from(env::var_os("HOME")?);
    home.is_absolute()
        .then(|| home.join(".config/to-view/saves/window"))
}

fn read_pair(path: &Path) -> Option<(i32, i32)> {
    let contents = fs::read_to_string(path).ok()?;
    let mut values = contents.split_whitespace();
    let first = values.next()?.parse().ok()?;
    let second = values.next()?.parse().ok()?;
    values.next().is_none().then_some((first, second))
}

pub fn add_ffplay_args(args: &mut Vec<Vec<u8>>) {
    let Some(dir) = directory() else { return };

    let position = read_pair(&dir.join("position.txt"));
    let size = read_pair(&dir.join("size.txt")).filter(|&(w, h)| w > 0 && h > 0);
    for (first_arg, second_arg, pair) in [
        (b"-left\0".as_slice(), b"-top\0".as_slice(), position),
        (b"-x\0".as_slice(), b"-y\0".as_slice(), size),
    ] {
        if let Some((first, second)) = pair {
            args.push(first_arg.to_vec());
            args.push(format!("{first}\0").into_bytes());
            args.push(second_arg.to_vec());
            args.push(format!("{second}\0").into_bytes());
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

#[unsafe(no_mangle)]
extern "C" fn to_view_save_window_geometry(x: c_int, y: c_int, w: c_int, h: c_int) {
    let Some(dir) = directory() else { return };
    if let Err(error) = fs::create_dir_all(&dir)
        .and_then(|()| write_pair(&dir, "position.txt", x, y))
        .and_then(|()| write_pair(&dir, "size.txt", w, h))
    {
        eprintln!(
            "to-view: could not save window geometry in {}: {error}",
            dir.display()
        );
    }
}
