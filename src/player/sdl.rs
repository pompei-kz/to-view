use super::{ffi::*, media};
use crate::window_geometry::Geometry;
use std::{
  ffi::CStr,
  path::PathBuf,
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
  SDL_KeyCode_SDLK_9 as NINE, SDL_KeyCode_SDLK_KP_DIVIDE as KP_DIVIDE,
  SDL_KeyCode_SDLK_KP_MULTIPLY as KP_MULTIPLY, SDL_KeyCode_SDLK_a as A, SDL_KeyCode_SDLK_c as C,
  SDL_KeyCode_SDLK_m as M, SDL_KeyCode_SDLK_p as P, SDL_KeyCode_SDLK_w as W,
};

use super::ffi::{
  SDL_EventType_SDL_KEYDOWN as KEY_DOWN, SDL_EventType_SDL_KEYMAPCHANGED as KEYMAP_CHANGED,
  SDL_EventType_SDL_MOUSEBUTTONDOWN as MOUSE_DOWN, SDL_EventType_SDL_MOUSEBUTTONUP as MOUSE_UP,
  SDL_EventType_SDL_MOUSEMOTION as MOUSE_MOTION, SDL_EventType_SDL_MOUSEWHEEL as MOUSE_WHEEL,
  SDL_EventType_SDL_QUIT as QUIT, SDL_EventType_SDL_WINDOWEVENT as WINDOW_EVENT,
  SDL_WindowEventID_SDL_WINDOWEVENT_FOCUS_LOST as FOCUS_LOST,
  SDL_WindowEventID_SDL_WINDOWEVENT_HIDDEN as HIDDEN,
  SDL_WindowEventID_SDL_WINDOWEVENT_MINIMIZED as MINIMIZED,
  SDL_WindowEventID_SDL_WINDOWEVENT_RESIZED as RESIZED,
  SDL_WindowEventID_SDL_WINDOWEVENT_SIZE_CHANGED as SIZE_CHANGED,
};

pub const LEFT_BUTTON: u8 = SDL_BUTTON_LEFT as u8;
pub const RIGHT_BUTTON: u8 = SDL_BUTTON_RIGHT as u8;
pub const MIDDLE_BUTTON: u8 = SDL_BUTTON_MIDDLE as u8;

pub const SHIFT: u16 = SDL_Keymod_KMOD_SHIFT as u16;
pub const CTRL: u16 = SDL_Keymod_KMOD_CTRL as u16;
pub const ALT: u16 = SDL_Keymod_KMOD_ALT as u16;
pub const Q: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_Q;
pub const F: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_F;
pub const V: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_V;
pub const SCAN_BACKSPACE: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_BACKSPACE;
pub const SCAN_S: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_S;
pub const SCAN_X: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_X;
pub const SCAN_C: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_C;
pub const SCAN_Z: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_Z;
pub const SCAN_0: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_0;
pub const SCAN_1: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_1;
pub const SCAN_8: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_8;
pub const SCAN_VOLUMEUP: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_VOLUMEUP;
pub const SCAN_VOLUMEDOWN: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_VOLUMEDOWN;
pub const SCAN_MUTE: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_MUTE;
pub const SCAN_AUDIO_MUTE: SDL_Scancode = SDL_Scancode_SDL_SCANCODE_AUDIOMUTE;

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
  MouseUp {
    button: u8,
  },
  Motion {
    x: i32,
    y: i32,
  },
  Wheel {
    x: i32,
    y: i32,
    delta: f64,
  },
  FocusLost,
  Resized,
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
  menu_key: i32,
  menu_toggle_key: i32,
  menu_frame: Option<u64>,
  menu_fit_hint: bool,
  menu_font: fontdue::Font,
  fallback_font: fontdue::Font,
  keyboard_layout: Option<super::keyboard_layout::KeyboardLayout>,
  video: *mut SDL_Texture,
  video_size: (i32, i32),
  geometry: Geometry,
  media_path: PathBuf,
  default_size_pending: bool,
  geometry_changed_at: Option<Instant>,
  visible: bool,
}

impl Ui {
  pub fn new(geometry: Geometry, media_path: PathBuf, file_name: &str) -> Result<Self, String> {
    unsafe {
      if SDL_Init(SDL_INIT_VIDEO | SDL_INIT_TIMER) != 0 {
        return Err(error());
      }
      let center = SDL_WINDOWPOS_CENTERED_MASK as i32;
      let (x, y) = geometry.position.unwrap_or((center, center));
      let (w, h) = geometry.size.unwrap_or((640, 480));
      let title = std::ffi::CString::new(format!("{file_name} - toview"))
        .map_err(|_| crate::i18n::tr("invalid window title"))?;
      let window = SDL_CreateWindow(
        title.as_ptr(),
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
      let menu_font = fontdue::Font::from_bytes(
        include_bytes!("../../assets/DejaVuSans.ttf") as &[u8],
        fontdue::FontSettings::default(),
      )
      .map_err(|error| crate::i18n::format("load menu font: {error}", &[("error", error)]))?;
      let fallback_font = fontdue::Font::from_bytes(
        include_bytes!("../../assets/DroidSansFallbackFull.ttf") as &[u8],
        fontdue::FontSettings::default(),
      )
      .map_err(|error| crate::i18n::format("load fallback font: {error}", &[("error", error)]))?;
      Ok(Self {
        window,
        renderer,
        menu: ptr::null_mut(),
        menu_size: (0, 0),
        menu_key: -1,
        menu_toggle_key: -1,
        menu_frame: None,
        menu_fit_hint: false,
        menu_font,
        fallback_font,
        keyboard_layout: if CStr::from_ptr(SDL_GetCurrentVideoDriver()) == c"x11" {
          super::keyboard_layout::KeyboardLayout::open()
        } else {
          None
        },
        video: ptr::null_mut(),
        video_size: (0, 0),
        geometry,
        media_path,
        default_size_pending: geometry.size.is_none(),
        geometry_changed_at: None,
        visible: false,
      })
    }
  }

  pub fn show(&mut self, size: Option<(i32, i32)>) {
    unsafe {
      if self.default_size_pending
        && let Some((video_width, video_height)) = size
      {
        let (mut mouse_x, mut mouse_y) = (0, 0);
        SDL_GetGlobalMouseState(&mut mouse_x, &mut mouse_y);
        let mut bounds = Rect {
          x: 0,
          y: 0,
          w: 0,
          h: 0,
        };
        let mut selected = SDL_GetWindowDisplayIndex(self.window).max(0);
        for index in 0..SDL_GetNumVideoDisplays() {
          let mut display = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
          };
          if SDL_GetDisplayBounds(index, &mut display) == 0
            && mouse_x >= display.x
            && mouse_y >= display.y
            && i64::from(mouse_x) < i64::from(display.x) + i64::from(display.w)
            && i64::from(mouse_y) < i64::from(display.y) + i64::from(display.h)
          {
            selected = index;
            break;
          }
        }
        if SDL_GetDisplayUsableBounds(selected, &mut bounds) != 0 {
          SDL_GetDisplayBounds(selected, &mut bounds);
        }
        if bounds.w > 0 && bounds.h > 0 && video_width > 0 && video_height > 0 {
          let (w, h) = initial_window_size(video_width, video_height, bounds.w, bounds.h);
          SDL_SetWindowSize(self.window, w, h);
          if self.geometry.position.is_none() {
            SDL_SetWindowPosition(
              self.window,
              bounds.x + (bounds.w - w) / 2,
              bounds.y + (bounds.h - h) / 2,
            );
          }
        }
        self.default_size_pending = false;
      }
      if self.visible {
        return;
      }
      SDL_ShowWindow(self.window);
      // The window starts hidden while its initial video-sized geometry is
      // calculated. Explicitly activate it after mapping; otherwise some X11
      // window managers leave focus on the previously active application.
      SDL_RaiseWindow(self.window);
      SDL_SetWindowInputFocus(self.window);
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
      self.geometry.save(&self.media_path);
    }
  }

  pub fn event(&mut self) -> Option<Event> {
    if self
      .keyboard_layout
      .as_mut()
      .is_some_and(|layout| layout.changed())
    {
      return Some(Event::Redraw);
    }
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
          KEYMAP_CHANGED => return Some(Event::Redraw),
          MOUSE_DOWN => {
            let button = event.button;
            return Some(Event::Mouse {
              x: button.x,
              y: button.y,
              button: button.button,
              clicks: button.clicks,
            });
          }
          MOUSE_UP => {
            return Some(Event::MouseUp {
              button: event.button.button,
            });
          }
          MOUSE_MOTION => {
            return Some(Event::Motion {
              x: event.motion.x,
              y: event.motion.y,
            });
          }
          MOUSE_WHEEL => {
            let wheel = event.wheel;
            let (x, y) = (wheel.mouseX, wheel.mouseY);
            let mut delta = if wheel.preciseY != 0.0 {
              f64::from(wheel.preciseY)
            } else {
              f64::from(wheel.y)
            };
            if wheel.direction == SDL_MouseWheelDirection_SDL_MOUSEWHEEL_FLIPPED {
              delta = -delta;
            }
            return Some(Event::Wheel { x, y, delta });
          }
          WINDOW_EVENT => {
            if event.window.event as u32 == SDL_WindowEventID_SDL_WINDOWEVENT_CLOSE {
              return Some(Event::Quit);
            }
            self.remember();
            match event.window.event as u32 {
              FOCUS_LOST | HIDDEN | MINIMIZED => return Some(Event::FocusLost),
              SIZE_CHANGED | RESIZED => return Some(Event::Resized),
              _ => return Some(Event::Redraw),
            }
          }
          _ => {}
        }
      }
    }
    None
  }

  pub fn capture_mouse(&self, capture: bool) {
    unsafe {
      SDL_CaptureMouse(if capture {
        SDL_bool_SDL_TRUE
      } else {
        SDL_bool_SDL_FALSE
      });
    }
  }

  pub fn double_click_interval(&self) -> Duration {
    unsafe {
      let hint = SDL_GetHint(c"SDL_MOUSE_DOUBLE_CLICK_TIME".as_ptr());
      if !hint.is_null() {
        if let Ok(value) = CStr::from_ptr(hint).to_string_lossy().parse::<u64>() {
          return Duration::from_millis(value.min(60_000));
        }
      }
    }
    Duration::from_millis(500)
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

  pub fn upload(
    &mut self,
    frame: &media::Video,
    picture: super::settings::PictureSettings,
  ) -> Result<(), String> {
    let pixels = picture.apply(&frame.pixels);
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
        pixels.as_ptr().cast(),
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

  pub fn background(&self, aspect: f64, rotation: f64, view: super::view::ViewTransform) {
    self.clear();
    unsafe {
      if self.video.is_null() {
        return;
      }
      let size = self.size();
      let Some((fit_w, fit_h, sideways)) = super::view::fit(size, aspect, rotation) else {
        return;
      };
      let (center_x, center_y) = view.center(size, (fit_w, fit_h));
      let (w, h) = if sideways {
        (fit_h * view.zoom, fit_w * view.zoom)
      } else {
        (fit_w * view.zoom, fit_h * view.zoom)
      };
      let dst = Rect {
        x: (center_x - w / 2.0).round() as i32,
        y: (center_y - h / 2.0).round() as i32,
        w: (w.round() as i32).max(1),
        h: (h.round() as i32).max(1),
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

  pub fn menu(&mut self, frame: Option<u64>, fit_hint: bool) {
    let key = self
      .keyboard_layout
      .as_ref()
      .and_then(|layout| layout.key(false))
      .unwrap_or_else(|| unsafe { SDL_GetKeyFromScancode(Q) });
    let toggle_key = self
      .keyboard_layout
      .as_ref()
      .and_then(|layout| layout.key(true))
      .unwrap_or_else(|| unsafe { SDL_GetKeyFromScancode(V) });
    if self.menu_key != key
      || self.menu_toggle_key != toggle_key
      || self.menu_frame != frame
      || self.menu_fit_hint != fit_hint
    {
      self.update_menu(key, toggle_key, frame, fit_hint);
    }
    let (width, _) = self.size();
    let (menu_w, menu_h) = self.menu_size;
    if width <= 74 || self.menu.is_null() || menu_w <= 0 {
      return;
    }
    let w = menu_w.min(width - 74);
    let dst = Rect {
      x: 12,
      y: 12,
      w,
      h: (menu_h * w / menu_w).max(1),
    };
    unsafe { SDL_RenderCopy(self.renderer, self.menu, ptr::null(), &dst) };
  }

  fn update_menu(&mut self, key: i32, toggle_key: i32, frame: Option<u64>, fit_hint: bool) {
    let label = char::from_u32(key as u32)
      .filter(|ch| !ch.is_control())
      .map(|ch| ch.to_uppercase().collect::<String>())
      .unwrap_or_else(|| "Q".to_owned());
    let toggle_label = char::from_u32(toggle_key as u32)
      .filter(|ch| !ch.is_control())
      .map(|ch| ch.to_uppercase().collect::<String>())
      .unwrap_or_else(|| "V".to_owned());
    let frame = frame.map_or_else(|| "-".to_owned(), |value| value.to_string());
    let mut text = format!(
      "{}\n{}\n{}",
      crate::i18n::format(
        "{quit} - quit, Esc / {menu} - Menu (show/hide)",
        &[("quit", &label), ("menu", &toggle_label)]
      ),
      crate::i18n::tr("Ctrl+Left / Ctrl+Right - frame by frame"),
      crate::i18n::format("Frame: {frame}", &[("frame", &frame)]),
    );
    if fit_hint {
      text.push('\n');
      text.push_str(&crate::i18n::tr("Backspace - fit to window"));
    }
    if let Some((texture, width, height)) = self.text_texture(&text) {
      unsafe {
        if !self.menu.is_null() {
          SDL_DestroyTexture(self.menu);
        }
      }
      self.menu = texture;
      self.menu_size = (width, height);
      self.menu_key = key;
      self.menu_toggle_key = toggle_key;
      self.menu_frame = frame.parse().ok();
      self.menu_fit_hint = fit_hint;
    }
  }

  fn text_texture(&self, text: &str) -> Option<(*mut SDL_Texture, i32, i32)> {
    use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings::default());
    let fonts = [&self.menu_font, &self.fallback_font];
    for ch in text.chars() {
      let mut buffer = [0; 4];
      let value = ch.encode_utf8(&mut buffer);
      let index = usize::from(
        self.menu_font.lookup_glyph_index(ch) == 0
          && self.fallback_font.lookup_glyph_index(ch) != 0,
      );
      layout.append(&fonts, &TextStyle::new(value, 18.0, index));
    }

    let width = layout
      .glyphs()
      .iter()
      .map(|g| g.x.ceil() as i32 + g.width as i32)
      .max()
      .unwrap_or(0)
      + 4;
    let height = layout
      .glyphs()
      .iter()
      .map(|g| g.y.ceil() as i32 + g.height as i32)
      .max()
      .unwrap_or(0)
      + 4;
    if width <= 0 || height <= 0 {
      return None;
    }
    let mut mask = vec![0u8; width as usize * height as usize];
    for glyph in layout.glyphs() {
      let (_, bitmap) = fonts[glyph.font_index].rasterize_config(glyph.key);
      for row in 0..glyph.height {
        for col in 0..glyph.width {
          let x = glyph.x as i32 + col as i32 + 2;
          let y = glyph.y as i32 + row as i32 + 2;
          if (0..width).contains(&x) && (0..height).contains(&y) {
            let index = (y * width + x) as usize;
            mask[index] = mask[index].max(bitmap[row * glyph.width + col]);
          }
        }
      }
    }
    let mut pixels = vec![0u8; mask.len() * 4];
    for y in 0..height {
      for x in 0..width {
        let index = (y * width + x) as usize;
        let mut outline = 0;
        for dy in -1..=1 {
          for dx in -1..=1 {
            let (nx, ny) = (x + dx, y + dy);
            if (0..width).contains(&nx) && (0..height).contains(&ny) {
              outline = outline.max(mask[(ny * width + nx) as usize]);
            }
          }
        }
        let foreground = mask[index] as u16;
        let alpha = outline.max(mask[index]) as u16;
        if alpha > 0 {
          let shade = (foreground * 255 / alpha) as u8;
          pixels[index * 4..index * 4 + 4].copy_from_slice(&[shade, shade, shade, alpha as u8]);
        }
      }
    }
    unsafe {
      let texture = SDL_CreateTexture(
        self.renderer,
        RGBA,
        SDL_TextureAccess_SDL_TEXTUREACCESS_STATIC as i32,
        width,
        height,
      );
      if texture.is_null() {
        return None;
      }
      if SDL_SetTextureBlendMode(texture, SDL_BlendMode_SDL_BLENDMODE_BLEND) != 0
        || SDL_UpdateTexture(texture, ptr::null(), pixels.as_ptr().cast(), width * 4) != 0
      {
        SDL_DestroyTexture(texture);
        return None;
      }
      Some((texture, width, height))
    }
  }

  fn key_label(&self, scan: SDL_Scancode, letter: u8, xcode: u8) -> String {
    let key = self
      .keyboard_layout
      .as_ref()
      .and_then(|layout| layout.physical_key(letter, xcode))
      .unwrap_or_else(|| unsafe { SDL_GetKeyFromScancode(scan) });
    char::from_u32(key as u32)
      .filter(|ch| !ch.is_control())
      .map(|ch| ch.to_uppercase().collect())
      .unwrap_or_else(|| (letter as char).to_ascii_uppercase().to_string())
  }

  pub fn extended_button_hit(&self, x: i32, y: i32) -> bool {
    let (w, _) = self.size();
    x >= (w - 62).max(0) && x < w && (0..58).contains(&y)
  }

  fn draw_text(&self, text: &str, x: i32, y: i32, max_width: i32) -> i32 {
    if max_width <= 0 {
      return 0;
    }
    if let Some((texture, width, height)) = self.text_texture(text) {
      let visible = width.min(max_width);
      let src = Rect {
        x: 0,
        y: 0,
        w: visible,
        h: height,
      };
      let dst = Rect {
        x,
        y,
        w: visible,
        h: height,
      };
      unsafe {
        SDL_RenderCopy(self.renderer, texture, &src, &dst);
        SDL_DestroyTexture(texture);
      }
      height
    } else {
      0
    }
  }

  fn draw_text_right(&self, text: &str, right: i32, y: i32, max_width: i32) {
    if max_width <= 0 {
      return;
    }
    if let Some((texture, width, height)) = self.text_texture(text) {
      let visible = width.min(max_width);
      let src = Rect {
        x: 0,
        y: 0,
        w: visible,
        h: height,
      };
      let dst = Rect {
        x: right - visible,
        y,
        w: visible,
        h: height,
      };
      unsafe {
        SDL_RenderCopy(self.renderer, texture, &src, &dst);
        SDL_DestroyTexture(texture);
      }
    }
  }

  pub fn extended_menu(&self, extended: bool, settings: super::settings::Settings) {
    let (width, height) = self.size();
    if width < 24 {
      return;
    }
    let x = (width - 46).max(1);
    self.color((0, 0, 0));
    for y in [18, 28, 38] {
      for d in -1..=1 {
        self.line(x, y + d, x + 32, y + d);
      }
    }
    self.color((255, 255, 255));
    for y in [18, 28, 38] {
      self.line(x, y, x + 32, y);
    }
    let label = self.key_label(SCAN_S, b's', 39);
    self.draw_text(&label, x + 9, 18, 26);
    if !extended {
      return;
    }
    let picture = settings.picture;
    let xkey = self.key_label(SCAN_X, b'x', 53);
    let ckey = self.key_label(SCAN_C, b'c', 54);
    let zkey = self.key_label(SCAN_Z, b'z', 52);
    let left = [
      crate::i18n::format(
        "Brightness: 1 - {value} + 2",
        &[("value", &picture.brightness.to_string())],
      ),
      crate::i18n::format(
        "Contrast: 3 - {value} + 4",
        &[("value", &picture.contrast.to_string())],
      ),
      crate::i18n::format(
        "Gamma: 5 - {value} + 6",
        &[("value", &picture.gamma.to_string())],
      ),
      crate::i18n::format(
        "Saturation: 7 - {value} + 8",
        &[("value", &picture.saturation.to_string())],
      ),
      crate::i18n::tr("0 - reset picture"),
    ];
    let right = [
      crate::i18n::format(
        "Volume: {down} - {volume}% + {up}",
        &[
          ("down", &xkey),
          ("volume", &settings.volume_text()),
          ("up", &ckey),
        ],
      ),
      crate::i18n::format(
        if settings.muted {
          "{key} - enable sound"
        } else {
          "{key} - mute sound"
        },
        &[("key", &zkey)],
      ),
    ];
    let start_y = if settings.view.zoom != 1.0 { 120 } else { 95 };
    let narrow = width < 700;
    let right_x = if narrow { 12 } else { width / 2 + 8 };
    let right_y = if narrow { start_y + 5 * 25 } else { start_y };
    let left_width = if narrow { width - 24 } else { width / 2 - 20 };
    for (i, line) in left.iter().enumerate() {
      let y = start_y + i as i32 * 25;
      if y + 22 < height - 24 {
        self.draw_text(line, 12, y, left_width);
      }
    }
    for (i, line) in right.iter().enumerate() {
      let y = right_y + i as i32 * 25;
      if y + 22 < height - 24 {
        self.draw_text_right(line, width - 12, y, width - right_x - 12);
      }
    }
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

fn initial_window_size(video_w: i32, video_h: i32, display_w: i32, display_h: i32) -> (i32, i32) {
  let max_w = (display_w * 4 / 5).max(1);
  let max_h = (display_h * 4 / 5).max(1);
  let max_scale =
    (f64::from(max_w) / f64::from(video_w)).min(f64::from(max_h) / f64::from(video_h));
  let scale = if video_w > max_w || video_h > max_h {
    max_scale
  } else if video_w < 100 || video_h < 100 {
    (100.0 / f64::from(video_w))
      .max(100.0 / f64::from(video_h))
      .min(max_scale)
  } else {
    1.0
  };
  (
    (f64::from(video_w) * scale).round().max(1.0) as i32,
    (f64::from(video_h) * scale).round().max(1.0) as i32,
  )
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
    let bytes = u32::try_from(std::mem::size_of_val(samples))
      .map_err(|_| crate::i18n::tr("audio packet is too large"))?;
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

#[cfg(test)]
mod tests {
  use super::initial_window_size;

  #[test]
  fn initial_size_keeps_native_size_and_respects_both_limits() {
    assert_eq!(initial_window_size(720, 304, 1920, 1080), (720, 304));
    assert_eq!(initial_window_size(1920, 804, 1920, 1080), (1536, 643));
    assert_eq!(initial_window_size(1080, 1920, 1920, 1080), (486, 864));
  }

  #[test]
  fn initial_size_enlarges_tiny_video_proportionally() {
    assert_eq!(initial_window_size(50, 40, 1920, 1080), (125, 100));
  }
}
