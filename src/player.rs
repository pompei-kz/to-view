use std::{
    ffi::{c_char, c_int, c_void},
    fs,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    ptr,
};

pub struct Options {
    pub path: PathBuf,
    pub close_on_end: bool,
}

pub fn play(options: Options) -> Result<u8, String> {
    let path = fs::canonicalize(&options.path)
        .map_err(|error| format!("{}: {error}", options.path.display()))?;
    if !path.is_file() {
        return Err(format!("{}: expected a regular video file", path.display()));
    }

    let mut args = vec![
        b"to-view\0".to_vec(),
        b"-hide_banner\0".to_vec(),
        b"-loglevel\0".to_vec(),
        b"error\0".to_vec(),
        b"-window_title\0".to_vec(),
        b"to-view\0".to_vec(),
    ];
    if options.close_on_end {
        args.push(b"-autoexit\0".to_vec());
    }
    crate::window_geometry::add_ffplay_args(&mut args);
    args.push([path.as_os_str().as_bytes(), b"\0"].concat());
    let mut argv: Vec<*mut c_char> = args.iter_mut().map(|arg| arg.as_mut_ptr().cast()).collect();
    argv.push(ptr::null_mut());

    // ffplay owns the event loop and normally terminates the process itself.
    // The argument buffers remain writable and valid for the entire call.
    let status = unsafe { to_view_ffplay_main(args.len() as c_int, argv.as_mut_ptr()) };
    Ok(u8::try_from(status).unwrap_or(1))
}

type Window = c_void;
type Surface = c_void;
type Stream = c_void;
type Pcm = c_void;
type AlsaConfig = c_void;

unsafe extern "C" {
    fn to_view_ffplay_main(argc: c_int, argv: *mut *mut c_char) -> c_int;
    fn __real_SDL_CreateWindow(
        title: *const c_char,
        x: c_int,
        y: c_int,
        w: c_int,
        h: c_int,
        flags: u32,
    ) -> *mut Window;
    fn SDL_RWFromConstMem(mem: *const c_void, size: c_int) -> *mut Stream;
    fn SDL_LoadBMP_RW(stream: *mut Stream, free_stream: c_int) -> *mut Surface;
    fn SDL_SetWindowIcon(window: *mut Window, surface: *mut Surface);
    fn SDL_FreeSurface(surface: *mut Surface);
    fn __real_snd_pcm_open(
        pcm: *mut *mut Pcm,
        name: *const c_char,
        stream: c_int,
        mode: c_int,
    ) -> c_int;
    fn snd_pcm_open_lconf(
        pcm: *mut *mut Pcm,
        name: *const c_char,
        stream: c_int,
        mode: c_int,
        config: *mut AlsaConfig,
    ) -> c_int;
    fn snd_config_load_string(
        config: *mut *mut AlsaConfig,
        text: *const c_char,
        size: usize,
    ) -> c_int;
    fn snd_card_next(card: *mut c_int) -> c_int;
    fn snd_config_delete(config: *mut AlsaConfig) -> c_int;
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __wrap_SDL_CreateWindow(
    title: *const c_char,
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
    flags: u32,
) -> *mut Window {
    const ICON: &[u8] = include_bytes!("../assets/icon.bmp");
    unsafe {
        let window = __real_SDL_CreateWindow(title, x, y, w, h, flags);
        if !window.is_null() {
            let icon = load_bmp(ICON);
            if !icon.is_null() {
                SDL_SetWindowIcon(window, icon);
                SDL_FreeSurface(icon);
            }
        }
        window
    }
}

fn load_bmp(bytes: &'static [u8]) -> *mut Surface {
    unsafe {
        let source = SDL_RWFromConstMem(bytes.as_ptr().cast(), bytes.len() as c_int);
        if source.is_null() {
            return ptr::null_mut();
        }
        SDL_LoadBMP_RW(source, 1)
    }
}

#[unsafe(no_mangle)]
extern "C" fn to_view_load_menu_label() -> *mut Surface {
    load_bmp(include_bytes!("../assets/menu-label.bmp"))
}

// ALSA can try the host's default PCM first. The embedded fallback can use
// a kernel audio device even when there is no system ALSA configuration on disk.
#[unsafe(no_mangle)]
unsafe extern "C" fn __wrap_snd_pcm_open(
    pcm: *mut *mut Pcm,
    name: *const c_char,
    stream: c_int,
    mode: c_int,
) -> c_int {
    unsafe {
        let result = __real_snd_pcm_open(pcm, name, stream, mode);
        if result >= 0 || std::ffi::CStr::from_ptr(name) != c"default" {
            return result;
        }
        let mut card = -1;
        while snd_card_next(&mut card) >= 0 && card >= 0 {
            let text = format!("pcm.default {{ type plug slave.pcm {{ type hw card {card} }} }}");
            let mut config = ptr::null_mut();
            if snd_config_load_string(&mut config, text.as_ptr().cast(), text.len()) < 0 {
                return result;
            }
            let status = snd_pcm_open_lconf(pcm, name, stream, mode, config);
            snd_config_delete(config);
            if status >= 0 {
                return status;
            }
        }
        result
    }
}

// Xlib optionally loads input-method modules. Refuse shared objects so the
// executable also stays independent of shared libraries after startup.
#[unsafe(no_mangle)]
extern "C" fn __wrap_dlopen(_filename: *const c_char, _flags: c_int) -> *mut c_void {
    ptr::null_mut()
}

#[unsafe(no_mangle)]
extern "C" fn __wrap___libc_dlopen_mode(filename: *const c_char, flags: c_int) -> *mut c_void {
    __wrap_dlopen(filename, flags)
}
