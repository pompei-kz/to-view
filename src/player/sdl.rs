use super::{ffi::*, media};
use crate::window_geometry::Geometry;
use std::{
  ffi::CStr,
  ptr,
  time::{Duration, Instant},
};

pub use super::ffi::{
  SDL_KeyCode_SDLK_DOWN as DOWN, SDL_KeyCode_SDLK_END as END, SDL_KeyCode_SDLK_ESCAPE as ESC,
  SDL_KeyCode_SDLK_HOME as HOME, SDL_KeyCode_SDLK_LEFT as LEFT,
  SDL_KeyCode_SDLK_PAGEDOWN as PAGE_DOWN, SDL_KeyCode_SDLK_PAGEUP as PAGE_UP,
  SDL_KeyCode_SDLK_RIGHT as RIGHT, SDL_KeyCode_SDLK_SPACE as SPACE, SDL_KeyCode_SDLK_UP as UP,
  SDL_Rect as Rect,
};

pub use super::ffi::{
  SDL_KeyCode_SDLK_0 as ZERO, SDL_KeyCode_SDLK_9 as NINE, SDL_KeyCode_SDLK_KP_DIVIDE as KP_DIVIDE,
  SDL_KeyCode_SDLK_KP_MULTIPLY as KP_MULTIPLY, SDL_KeyCode_SDLK_a as A, SDL_KeyCode_SDLK_c as C,
  SDL_KeyCode_SDLK_f as F, SDL_KeyCode_SDLK_m as M, SDL_KeyCode_SDLK_p as P,
  SDL_KeyCode_SDLK_s as S, SDL_KeyCode_SDLK_v as V, SDL_KeyCode_SDLK_w as W,
};

use super::ffi::{
  SDL_EventType_SDL_KEYDOWN as KEY_DOWN, SDL_EventType_SDL_MOUSEBUTTONDOWN as MOUSE_DOWN,
  SDL_EventType_SDL_MOUSEMOTION as MOUSE_MOTION, SDL_EventType_SDL_QUIT as QUIT,
  SDL_EventType_SDL_WINDOWEVENT as WINDOW_EVENT,
};

pub const LEFT_BUTTON: u8 = SDL_BUTTON_LEFT as u8;
pub const RIGHT_BUTTON: u8 = SDL_BUTTON_RIGHT as u8;
const RIGHT_BUTTON_MASK: u32 = 1 << (RIGHT_BUTTON - 1);

pub const SHIFT: u16 = SDL_Keymod_KMOD_SHIFT as u16;
pub const ALT: u16 = SDL_Keymod_KMOD_ALT as u16;
pub const Q: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_Q;

#[cfg(target_endian = "little")]
const RGBA: u32 = SDL_PixelFormatEnum_SDL_PIXELFORMAT_ABGR8888;
#[cfg(target_endian = "big")]
const RGBA: u32 = SDL_PixelFormatEnum_SDL_PIXELFORMAT_RGBA8888;

pub enum Event {
  Quit,
  Key {
    code: i32,
    scan: SDL_Scancode,
    mods: u16,
    repeat: bool,
  },
  Mouse {
    x: i32,
    y: i32,
    button: u8,
    clicks: u8,
  },
  RightDrag {
    x: i32,
    y: i32,
  },
  Redraw,
}

fn error() -> String {
  unsafe {
    CStr::from_ptr(SDL_GetError())
      .to_string_lossy()
      .into_owned()
  }
}

pub struct Ui {
  window: *mut SDL_Window,
  renderer: *mut SDL_Renderer,
  menu: *mut SDL_Texture,
  menu_size: (i32, i32),
  video: *mut SDL_Texture,
  video_size: (i32, i32),
  geometry: Geometry,
  geometry_changed_at: Option<Instant>,
  saved_size: bool,
  visible: bool,
}

impl Ui {
  pub fn new(geometry: Geometry) -> Result<Self, String> {
    unsafe {
      if SDL_Init(SDL_INIT_VIDEO | SDL_INIT_TIMER) != 0 {
        return Err(error());
      }
      let center = SDL_WINDOWPOS_CENTERED_MASK as i32;
      let (x, y) = geometry.position.unwrap_or((center, center));
      let (w, h) = geometry.size.unwrap_or((640, 480));
      let window = SDL_CreateWindow(
        c"to-view".as_ptr(),
        x,
        y,
        w,
        h,
        SDL_WindowFlags_SDL_WINDOW_HIDDEN | SDL_WindowFlags_SDL_WINDOW_RESIZABLE,
      );
      if window.is_null() {
        let message = error();
        SDL_Quit();
        return Err(message);
      }
      let renderer = SDL_CreateRenderer(window, -1, 0);
      if renderer.is_null() {
        let message = error();
        SDL_DestroyWindow(window);
        SDL_Quit();
        return Err(message);
      }
      let icon = load_bmp(include_bytes!("../../assets/icon.bmp"));
      if !icon.is_null() {
        SDL_SetWindowIcon(window, icon);
        SDL_FreeSurface(icon);
      }
      let surface = load_bmp(include_bytes!("../../assets/menu-label.bmp"));
      let mut menu_size = (0, 0);
      let menu = if surface.is_null() {
        ptr::null_mut()
      } else {
        menu_size = ((*surface).w, (*surface).h);
        let texture = SDL_CreateTextureFromSurface(renderer, surface);
        SDL_FreeSurface(surface);
        texture
      };
      Ok(Self {
        window,
        renderer,
        menu,
        menu_size,
        video: ptr::null_mut(),
        video_size: (0, 0),
        geometry,
        geometry_changed_at: None,
        saved_size: geometry.size.is_some(),
        visible: false,
      })
    }
  }

  pub fn show(&mut self, size: Option<(i32, i32)>) {
    if self.visible {
      return;
    }
    unsafe {
      if let Some((w, h)) = size.filter(|_| !self.saved_size) {
        SDL_SetWindowSize(self.window, w, h);
      }
      SDL_ShowWindow(self.window);
    }
    self.visible = true;
    self.remember();
  }

  fn remember(&mut self) {
    if !self.visible {
      return;
    }
    unsafe {
      let flags = SDL_GetWindowFlags(self.window);
      if flags
        & (SDL_WindowFlags_SDL_WINDOW_FULLSCREEN
          | SDL_WindowFlags_SDL_WINDOW_MAXIMIZED
          | SDL_WindowFlags_SDL_WINDOW_MINIMIZED)
        != 0
      {
        return;
      }
      let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
      SDL_GetWindowPosition(self.window, &mut x, &mut y);
      SDL_GetWindowSize(self.window, &mut w, &mut h);
      if w > 0 && h > 0 {
        let geometry = Geometry {
          position: Some((x, y)),
          size: Some((w, h)),
        };
        if self.geometry != geometry {
          self.geometry = geometry;
          self.geometry_changed_at = Some(Instant::now());
        }
      }
    }
  }

  pub fn save_geometry_if_due(&mut self) {
    if self
      .geometry_changed_at
      .is_some_and(|changed| changed.elapsed() >= Duration::from_millis(300))
    {
      self.geometry_changed_at = None;
      self.geometry.save();
    }
  }

  pub fn event(&mut self) -> Option<Event> {
    unsafe {
      let mut event = std::mem::zeroed::<SDL_Event>();
      while SDL_PollEvent(&mut event) != 0 {
        match event.type_ {
          QUIT => return Some(Event::Quit),
          KEY_DOWN => {
            let key = event.key;
            return Some(Event::Key {
              code: key.keysym.sym,
              scan: key.keysym.scancode,
              mods: key.keysym.mod_,
              repeat: key.repeat != 0,
            });
          }
          MOUSE_DOWN => {
            let button = event.button;
            return Some(Event::Mouse {
              x: button.x,
              y: button.y,
              button: button.button,
              clicks: button.clicks,
            });
          }
          MOUSE_MOTION => {
            let motion = event.motion;
            if motion.state & RIGHT_BUTTON_MASK != 0 {
              return Some(Event::RightDrag {
                x: motion.x,
                y: motion.y,
              });
            }
            return Some(Event::Redraw);
          }
          WINDOW_EVENT => {
            if event.window.event as u32 == SDL_WindowEventID_SDL_WINDOWEVENT_CLOSE {
              return Some(Event::Quit);
            }
            self.remember();
            return Some(Event::Redraw);
          }
          _ => {}
        }
      }
    }
    None
  }

  pub fn fullscreen(&mut self) {
    self.remember();
    unsafe {
      let current = SDL_GetWindowFlags(self.window);
      let next = if current & SDL_WindowFlags_SDL_WINDOW_FULLSCREEN != 0 {
        0
      } else {
        SDL_WindowFlags_SDL_WINDOW_FULLSCREEN_DESKTOP
      };
      SDL_SetWindowFullscreen(self.window, next);
    }
  }

  pub fn size(&self) -> (i32, i32) {
    let (mut w, mut h) = (0, 0);
    unsafe { SDL_GetRendererOutputSize(self.renderer, &mut w, &mut h) };
    (w, h)
  }

  pub fn mouse(&self) -> Option<(i32, i32)> {
    unsafe {
      if SDL_GetMouseFocus() != self.window {
        return None;
      }
      let (mut x, mut y) = (0, 0);
      SDL_GetMouseState(&mut x, &mut y);
      Some(self.render_point(x, y))
    }
  }

  pub fn render_point(&self, x: i32, y: i32) -> (i32, i32) {
    let (mut window_w, mut window_h) = (0, 0);
    unsafe { SDL_GetWindowSize(self.window, &mut window_w, &mut window_h) };
    let (w, h) = self.size();
    let scale = |p: i32, src: i32, dst: i32| {
      if src <= 1 {
        p
      } else {
        (i64::from(p) * i64::from(dst - 1) / i64::from(src - 1)) as i32
      }
    };
    (scale(x, window_w, w), scale(y, window_h, h))
  }

  pub fn upload(&mut self, frame: &media::Video) -> Result<(), String> {
    unsafe {
      if self.video_size != (frame.width, frame.height) {
        if !self.video.is_null() {
          SDL_DestroyTexture(self.video);
        }
        self.video = SDL_CreateTexture(
          self.renderer,
          RGBA,
          SDL_TextureAccess_SDL_TEXTUREACCESS_STREAMING as i32,
          frame.width,
          frame.height,
        );
        if self.video.is_null() {
          return Err(error());
        }
        self.video_size = (frame.width, frame.height);
      }
      if SDL_UpdateTexture(
        self.video,
        ptr::null(),
        frame.pixels.as_ptr().cast(),
        frame.width * 4,
      ) != 0
      {
        return Err(error());
      }
    }
    Ok(())
  }

  pub fn clear(&self) {
    self.color((0, 0, 0));
    unsafe { SDL_RenderClear(self.renderer) };
  }

  pub fn line(&self, x: i32, y: i32, next_x: i32, next_y: i32) {
    unsafe { SDL_RenderDrawLine(self.renderer, x, y, next_x, next_y) };
  }

  pub fn background(&self, aspect: f64, rotation: f64) {
    self.clear();
    unsafe {
      if self.video.is_null() {
        return;
      }
      let (width, height) = self.size();
      let sideways =
        rotation.abs().rem_euclid(180.0) > 45.0 && rotation.abs().rem_euclid(180.0) < 135.0;
      let aspect = if sideways { 1.0 / aspect } else { aspect };
      let w = width.min((height as f64 * aspect).round() as i32).max(1);
      let h = height.min((width as f64 / aspect).round() as i32).max(1);
      let (w, h) = if sideways { (h, w) } else { (w, h) };
      let dst = Rect {
        x: (width - w) / 2,
        y: (height - h) / 2,
        w,
        h,
      };
      SDL_RenderCopyEx(
        self.renderer,
        self.video,
        ptr::null(),
        &dst,
        rotation,
        ptr::null(),
        SDL_RendererFlip_SDL_FLIP_NONE,
      );
    }
  }

  pub fn menu(&self) {
    let (width, _) = self.size();
    let (menu_w, menu_h) = self.menu_size;
    if width <= 24 || self.menu.is_null() {
      return;
    }
    let w = menu_w.min(width - 24);
    let dst = Rect {
      x: 12,
      y: 12,
      w,
      h: (menu_h * w / menu_w).max(1),
    };
    unsafe { SDL_RenderCopy(self.renderer, self.menu, ptr::null(), &dst) };
  }

  pub fn color(&self, (r, g, b): (u8, u8, u8)) {
    unsafe { SDL_SetRenderDrawColor(self.renderer, r, g, b, 255) };
  }

  pub fn outline(&self, rect: Rect) {
    unsafe { SDL_RenderDrawRect(self.renderer, &rect) };
  }

  pub fn fill(&self, rect: Rect) {
    unsafe { SDL_RenderFillRect(self.renderer, &rect) };
  }

  pub fn fill_many(&self, rects: &[Rect]) {
    unsafe { SDL_RenderFillRects(self.renderer, rects.as_ptr(), rects.len() as i32) };
  }

  pub fn present(&self) {
    unsafe { SDL_RenderPresent(self.renderer) };
  }
}

impl Drop for Ui {
  fn drop(&mut self) {
    unsafe {
      SDL_DestroyRenderer(self.renderer);
      SDL_DestroyWindow(self.window);
      SDL_Quit();
    }
  }
}

unsafe fn load_bmp(bytes: &'static [u8]) -> *mut SDL_Surface {
  unsafe {
    let source = SDL_RWFromConstMem(bytes.as_ptr().cast(), bytes.len() as i32);
    if source.is_null() {
      ptr::null_mut()
    } else {
      SDL_LoadBMP_RW(source, 1)
    }
  }
}

pub struct AudioDevice {
  id: SDL_AudioDeviceID,
  pub latency: i64,
}

impl AudioDevice {
  pub fn open() -> Result<Self, String> {
    unsafe {
      if SDL_InitSubSystem(SDL_INIT_AUDIO) != 0 {
        return Err(error());
      }
      let mut spec = std::mem::zeroed::<SDL_AudioSpec>();
      spec.freq = media::SAMPLE_RATE;
      spec.format = AUDIO_F32SYS as u16;
      spec.channels = media::CHANNELS as u8;
      spec.samples = 1024;
      let mut obtained = std::mem::zeroed::<SDL_AudioSpec>();
      let id = SDL_OpenAudioDevice(ptr::null(), 0, &spec, &mut obtained, 0);
      if id == 0 {
        return Err(error());
      }
      Ok(Self {
        id,
        latency: i64::from(obtained.samples) * 1_000_000 / i64::from(media::SAMPLE_RATE),
      })
    }
  }

  pub fn pause(&self, paused: bool) {
    unsafe { SDL_PauseAudioDevice(self.id, i32::from(paused)) };
  }

  pub fn clear(&self) {
    unsafe { SDL_ClearQueuedAudio(self.id) };
  }

  pub fn queued(&self) -> i64 {
    i64::from(unsafe { SDL_GetQueuedAudioSize(self.id) }) * 1_000_000
      / (i64::from(media::SAMPLE_RATE) * media::CHANNELS as i64 * 4)
  }

  pub fn queue(&self, samples: &[f32]) -> Result<(), String> {
    let bytes =
      u32::try_from(std::mem::size_of_val(samples)).map_err(|_| "audio packet is too large")?;
    if unsafe { SDL_QueueAudio(self.id, samples.as_ptr().cast(), bytes) } != 0 {
      return Err(error());
    }
    Ok(())
  }
}

impl Drop for AudioDevice {
  fn drop(&mut self) {
    unsafe { SDL_CloseAudioDevice(self.id) };
  }
}
