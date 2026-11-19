use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};

const CORE_KEYBOARD: c_uint = 0x100;
const STATE_NOTIFY: c_uint = 2;
const GROUP_STATE_MASK: c_ulong = 1 << 4;

#[repr(C)]
#[derive(Default)]
struct State {
  group: u8,
  locked_group: u8,
  base_group: u16,
  latched_group: u16,
  mods: u8,
  base_mods: u8,
  latched_mods: u8,
  locked_mods: u8,
  compat_state: u8,
  grab_mods: u8,
  compat_grab_mods: u8,
  lookup_mods: u8,
  compat_lookup_mods: u8,
  ptr_buttons: u16,
}

#[repr(C)]
struct XkbEvent {
  event_type: c_int,
  serial: c_ulong,
  send_event: c_int,
  display: *mut c_void,
  time: c_ulong,
  xkb_type: c_int,
}

unsafe extern "C" {
  fn SDL_KeySymToUcs4(keysym: u32) -> u32;
  fn XOpenDisplay(name: *const c_char) -> *mut c_void;
  fn XCloseDisplay(display: *mut c_void) -> c_int;
  fn XPending(display: *mut c_void) -> c_int;
  fn XNextEvent(display: *mut c_void, event: *mut c_long) -> c_int;
  fn XKeysymToKeycode(display: *mut c_void, keysym: c_ulong) -> u8;
  fn XkbQueryExtension(
    display: *mut c_void,
    opcode: *mut c_int,
    event: *mut c_int,
    error: *mut c_int,
    major: *mut c_int,
    minor: *mut c_int,
  ) -> c_int;
  fn XkbSelectEventDetails(
    display: *mut c_void,
    device: c_uint,
    event: c_uint,
    affect: c_ulong,
    details: c_ulong,
  ) -> c_int;
  fn XkbGetState(display: *mut c_void, device: c_uint, state: *mut State) -> c_int;
  fn XkbKeycodeToKeysym(display: *mut c_void, keycode: u8, group: c_int, level: c_int) -> c_ulong;
}

pub struct KeyboardLayout {
  display: *mut c_void,
  event_base: c_int,
  keycode: u8,
  menu_keycode: u8,
}

impl KeyboardLayout {
  pub fn open() -> Option<Self> {
    unsafe {
      let display = XOpenDisplay(std::ptr::null());
      if display.is_null() {
        return None;
      }
      let (mut opcode, mut event, mut error, mut major, mut minor) = (0, 0, 0, 1, 0);
      if XkbQueryExtension(
        display,
        &mut opcode,
        &mut event,
        &mut error,
        &mut major,
        &mut minor,
      ) == 0
        || XkbSelectEventDetails(
          display,
          CORE_KEYBOARD,
          STATE_NOTIFY,
          GROUP_STATE_MASK,
          GROUP_STATE_MASK,
        ) == 0
      {
        XCloseDisplay(display);
        return None;
      }
      let mut keycode = XKeysymToKeycode(display, c_ulong::from(b'q'));
      if keycode == 0 {
        keycode = XKeysymToKeycode(display, 0x06ca); // Cyrillic short i, Й.
      }
      if keycode == 0 {
        keycode = 24;
      }
      let mut menu_keycode = XKeysymToKeycode(display, c_ulong::from(b'v'));
      if menu_keycode == 0 {
        menu_keycode = XKeysymToKeycode(display, 0x06cd);
      }
      if menu_keycode == 0 {
        menu_keycode = 55;
      }
      Some(Self {
        menu_keycode,
        display,
        event_base: event,
        keycode,
      })
    }
  }

  pub fn changed(&mut self) -> bool {
    let mut changed = false;
    unsafe {
      while XPending(self.display) > 0 {
        // XEvent is defined as a union of 24 longs by Xlib.
        let mut event = [0 as c_long; 24];
        XNextEvent(self.display, event.as_mut_ptr());
        let event = &*event.as_ptr().cast::<XkbEvent>();
        if event.event_type == self.event_base && event.xkb_type == STATE_NOTIFY as c_int {
          changed = true;
        }
      }
    }
    changed
  }

  pub fn key(&self, menu: bool) -> Option<i32> {
    unsafe {
      let mut state = State::default();
      if XkbGetState(self.display, CORE_KEYBOARD, &mut state) != 0 {
        return None;
      }
      let symbol = XkbKeycodeToKeysym(
        self.display,
        if menu {
          self.menu_keycode
        } else {
          self.keycode
        },
        state.group.into(),
        0,
      );
      let unicode = SDL_KeySymToUcs4(symbol as u32);
      (unicode != 0).then_some(unicode as i32)
    }
  }
}

impl Drop for KeyboardLayout {
  fn drop(&mut self) {
    unsafe {
      XCloseDisplay(self.display);
    }
  }
}
