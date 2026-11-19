use std::{
  ffi::{c_char, c_int, c_void},
  ptr,
};

type Pcm = c_void;
type AlsaConfig = c_void;

unsafe extern "C" {
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
