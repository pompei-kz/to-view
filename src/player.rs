mod audio_output;
mod ffi;
mod keyboard_layout;
mod media;
mod platform;
mod sdl;
mod seekbar;
mod settings;
mod view;
mod visualizer;

use crate::window_geometry::Geometry;
use media::{Event, Message, Seek, Target, TrackChange};
use std::{
  collections::VecDeque,
  ffi::CString,
  fs,
  os::unix::ffi::OsStrExt,
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender, TryRecvError},
  },
  thread,
  time::{Duration, Instant},
};

const VIDEO_QUEUE_FRAMES: usize = 16;
const AUDIO_QUEUE_MICROS: i64 = 500_000;

pub struct Options {
  pub path: PathBuf,
  pub close_on_end: bool,
}

pub fn play(options: Options) -> Result<u8, String> {
  let path = fs::canonicalize(&options.path)
    .map_err(|error| format!("{}: {error}", options.path.display()))?;
  if !path.is_file() {
    return Err(crate::i18n::format(
      "{path}: expected a regular video file",
      &[("path", &path.display().to_string())],
    ));
  }
  let input =
    CString::new(path.as_os_str().as_bytes()).map_err(|_| crate::i18n::tr("invalid file path"))?;
  let title = options
    .path
    .file_name()
    .unwrap_or(options.path.as_os_str())
    .to_string_lossy();
  let mut ui = sdl::Ui::new(Geometry::load(&path), path.clone(), &title)?;
  let (event_tx, event_rx) = mpsc::sync_channel(4);
  let (seek_tx, seek_rx) = mpsc::channel();
  let requested = Arc::new(AtomicU64::new(0));
  let worker_request = Arc::clone(&requested);
  let worker = thread::Builder::new()
    .name("media decoder".into())
    .spawn(move || media::run(input, worker_request, seek_rx, event_tx))
    .map_err(|e| {
      crate::i18n::format("start media decoder: {error}", &[("error", &e.to_string())])
    })?;
  let mut playback = Playback::new(requested, seek_tx, event_rx, path);
  let result = playback.run(&mut ui, options.close_on_end);
  playback.requested.store(media::STOP, Ordering::Release);
  drop(playback);
  worker
    .join()
    .map_err(|_| crate::i18n::tr("media decoder stopped unexpectedly"))?;
  result.map(|()| 0)
}

struct Clock {
  position: i64,
  updated: Instant,
}

impl Clock {
  fn set(&mut self, position: i64) {
    self.position = position;
    self.updated = Instant::now();
  }

  fn running(&self) -> i64 {
    self
      .position
      .saturating_add(self.updated.elapsed().as_micros() as i64)
  }
}

struct Playback {
  requested: Arc<AtomicU64>,
  commands: Sender<Seek>,
  events: Receiver<Message>,
  deferred: Option<Message>,
  metadata: Option<media::Metadata>,
  generation: u64,
  video: VecDeque<media::Video>,
  audio: Option<audio_output::Output>,
  audio_tail: Option<i64>,
  audio_drained_at: Option<Instant>,
  clock: Clock,
  position: i64,
  seek_target: Option<Target>,
  frame_end: i64,
  displayed_frame: Option<i64>,
  frame_number: Option<u64>,
  aspect: f64,
  paused: bool,
  started: bool,
  eof: bool,
  ended: bool,
  stepping: bool,
  menu: bool,
  extended_menu: bool,
  redraw: bool,
  settings: settings::Settings,
  media_path: PathBuf,
  displayed_video: Option<media::Video>,
  picture_dirty: bool,
  drag_position: Option<(i32, i32)>,
  pending_click: Option<Instant>,
  visualization: visualizer::Mode,
  samples: Vec<f32>,
}

impl Playback {
  fn new(
    requested: Arc<AtomicU64>,
    commands: Sender<Seek>,
    events: Receiver<Message>,
    media_path: PathBuf,
  ) -> Self {
    let settings = settings::Settings::load(&media_path);
    Self {
      requested,
      commands,
      events,
      deferred: None,
      metadata: None,
      generation: 0,
      video: VecDeque::new(),
      audio: None,
      audio_tail: None,
      audio_drained_at: None,
      clock: Clock {
        position: 0,
        updated: Instant::now(),
      },
      position: 0,
      seek_target: None,
      frame_end: 0,
      displayed_frame: None,
      frame_number: None,
      aspect: 1.0,
      paused: false,
      started: false,
      eof: false,
      ended: false,
      stepping: false,
      menu: false,
      extended_menu: false,
      redraw: true,
      settings,
      media_path,
      displayed_video: None,
      picture_dirty: false,
      drag_position: None,
      pending_click: None,
      visualization: visualizer::Mode::Video,
      samples: Vec::new(),
    }
  }

  fn duration(&self) -> Option<i64> {
    self.metadata.as_ref().and_then(|media| media.duration)
  }

  fn run(&mut self, ui: &mut sdl::Ui, close_on_end: bool) -> Result<(), String> {
    let opening = Instant::now();
    loop {
      while let Some(event) = ui.event() {
        if self.input(ui, event) {
          return Ok(());
        }
      }
      if self
        .pending_click
        .is_some_and(|deadline| Instant::now() >= deadline)
      {
        self.pending_click = None;
        self.toggle_playback();
        self.redraw = true;
      }
      ui.save_geometry_if_due();
      self.receive()?;
      self.update_clock();
      if let Some(frame) = self.next_frame() {
        let rotation = self.metadata.as_ref().map_or(0.0, |media| media.rotation);
        let display_width = (frame.height as f64 * frame.aspect)
          .round()
          .clamp(1.0, 8192.0) as i32;
        let size =
          if rotation.abs().rem_euclid(180.0) > 45.0 && rotation.abs().rem_euclid(180.0) < 135.0 {
            (frame.height, display_width)
          } else {
            (display_width, frame.height)
          };
        ui.show(Some(size));
        ui.upload(&frame, self.settings.picture)?;
        self.picture_dirty = false;
        self.aspect = frame.aspect;
        self.displayed_frame = Some(frame.pts);
        self.frame_number = Some(frame.number);
        self.frame_end = frame.pts.saturating_add(frame.duration);
        self.displayed_video = Some(frame);
        self.redraw = true;
      }
      if self.metadata.as_ref().is_some_and(|media| !media.has_video)
        || (!self.started && opening.elapsed() >= Duration::from_millis(300))
      {
        ui.show(None);
      }
      if self.finish(close_on_end)? {
        return Ok(());
      }
      if self.picture_dirty {
        if let Some(frame) = &self.displayed_video {
          ui.upload(frame, self.settings.picture)?;
        }
        self.picture_dirty = false;
      }
      if self.redraw {
        if self.visualization == visualizer::Mode::Video {
          ui.background(
            self.aspect,
            self.metadata.as_ref().map_or(0.0, |media| media.rotation),
            self.settings.view,
          );
        } else {
          ui.clear();
          visualizer::draw(ui, self.visualization, &self.samples);
        }
        if self.menu {
          ui.menu(self.frame_number, self.settings.view.zoom != 1.0);
          ui.extended_menu(self.extended_menu, self.settings);
          seekbar::draw(
            ui,
            self.display_position(),
            self.duration(),
            self.paused || self.ended,
          );
        }
        ui.present();
        self.redraw = false;
      }
      thread::sleep(Duration::from_millis(4));
    }
  }

  fn input(&mut self, ui: &mut sdl::Ui, event: sdl::Event) -> bool {
    self.redraw = true;
    if let sdl::Event::Key {
      scan, repeat, mods, ..
    } = &event
    {
      let (scan, repeat) = (*scan, *repeat);
      if scan == sdl::SCAN_S {
        if !repeat {
          self.extended_menu = !self.extended_menu;
        }
        return false;
      }
      if *mods & (sdl::CTRL | sdl::ALT) == 0 {
        let before = self.settings;
        if (sdl::SCAN_1..=sdl::SCAN_8).contains(&scan) {
          let n = scan - sdl::SCAN_1;
          let step = if n % 2 == 0 { -1 } else { 1 };
          let field = match n / 2 {
            0 => &mut self.settings.picture.brightness,
            1 => &mut self.settings.picture.contrast,
            2 => &mut self.settings.picture.gamma,
            _ => &mut self.settings.picture.saturation,
          };
          *field = (*field + step).clamp(-100, 100);
        } else if scan == sdl::SCAN_0 {
          self.settings.picture = settings::PictureSettings::default();
        } else if scan == sdl::SCAN_BACKSPACE {
          self.settings.view = view::ViewTransform::default();
        } else if scan == sdl::SCAN_X || scan == sdl::SCAN_VOLUMEDOWN {
          self.settings.volume = (self.settings.volume - 25).max(0);
        } else if scan == sdl::SCAN_C || scan == sdl::SCAN_VOLUMEUP {
          self.settings.volume = (self.settings.volume + 25).min(1000);
        } else if scan == sdl::SCAN_Z || scan == sdl::SCAN_MUTE || scan == sdl::SCAN_AUDIO_MUTE {
          if !repeat {
            self.settings.muted = !self.settings.muted;
          }
        } else { /* process other shortcuts below */
        }
        if self.settings != before {
          self.picture_dirty |= self.settings.picture != before.picture;
          self.save_settings();
        }
        if (sdl::SCAN_1..=sdl::SCAN_8).contains(&scan)
          || [
            sdl::SCAN_0,
            sdl::SCAN_BACKSPACE,
            sdl::SCAN_X,
            sdl::SCAN_C,
            sdl::SCAN_Z,
            sdl::SCAN_VOLUMEUP,
            sdl::SCAN_VOLUMEDOWN,
            sdl::SCAN_MUTE,
            sdl::SCAN_AUDIO_MUTE,
          ]
          .contains(&scan)
        {
          return false;
        }
      }
    }
    match event {
      sdl::Event::Quit => return true,
      sdl::Event::Key { scan: sdl::Q, .. } => return true,
      sdl::Event::Key {
        scan: sdl::V,
        repeat: false,
        ..
      } => self.menu = !self.menu,
      sdl::Event::Key { scan: sdl::V, .. } => {}
      sdl::Event::Key {
        scan: sdl::F,
        repeat: false,
        ..
      } => {
        self.pending_click = None;
        ui.fullscreen();
      }
      sdl::Event::Key { scan: sdl::F, .. } => {}
      sdl::Event::Key {
        code, mods, repeat, ..
      } => match code as u32 {
        sdl::ESC if !repeat => self.menu = !self.menu,
        sdl::SPACE | sdl::P if !repeat => self.pause(!self.paused),
        sdl::M if !repeat => {
          self.settings.muted = !self.settings.muted;
          self.save_settings();
        }
        sdl::W if !repeat => {
          if let Some(media) = &self.metadata
            && media.has_audio
          {
            self.visualization = self.visualization.next(media.has_video);
          }
        }
        sdl::KP_MULTIPLY => {
          self.settings.volume = (self.settings.volume + 25).min(1000);
          self.save_settings();
        }
        sdl::NINE | sdl::KP_DIVIDE => {
          self.settings.volume = (self.settings.volume - 25).max(0);
          self.save_settings();
        }
        sdl::HOME => self.seek(Target::First),
        sdl::END => self.seek(Target::Last),
        sdl::LEFT | sdl::RIGHT if mods & sdl::CTRL != 0 => {
          if self.paused || self.ended {
            if let Some(current) = self.displayed_frame {
              self.pause(true);
              self.seek(Target::Frame {
                current,
                forward: code as u32 == sdl::RIGHT,
              });
            }
          }
        }
        sdl::LEFT | sdl::RIGHT => {
          let seconds = if mods & sdl::SHIFT == 0 {
            1
          } else if mods & sdl::ALT != 0 {
            30
          } else {
            5
          };
          self.relative(if code as u32 == sdl::LEFT {
            -seconds
          } else {
            seconds
          });
        }
        sdl::DOWN => self.relative(-60),
        sdl::UP => self.relative(60),
        sdl::PAGE_DOWN => self.chapter(false),
        sdl::PAGE_UP => self.chapter(true),
        sdl::A if !repeat => self.cycle_tracks(true, false),
        sdl::C if !repeat && mods & sdl::CTRL != 0 => self.cycle_tracks(true, true),
        _ => {}
      },
      sdl::Event::Mouse {
        x,
        y,
        button: sdl::LEFT_BUTTON,
        clicks,
      } => {
        let (x, y) = ui.render_point(x, y);
        let button = self.menu && seekbar::is_button(ui, x, y);
        let expanded = self.menu && ui.extended_button_hit(x, y);
        let fraction = if self.menu {
          seekbar::fraction(ui, x, y)
        } else {
          None
        };
        let content = !button && !expanded && fraction.is_none();
        let is_double = content
          && clicks % 2 == 0
          && self
            .pending_click
            .is_some_and(|deadline| Instant::now() < deadline);
        if self.pending_click.is_some() && !is_double {
          self.pending_click = None;
          self.toggle_playback();
        }
        if expanded {
          self.extended_menu = !self.extended_menu;
        } else if button {
          self.toggle_playback();
        } else if let Some(fraction) = fraction {
          self.seek_fraction(fraction);
        } else if is_double {
          self.pending_click = None;
          ui.fullscreen();
        } else {
          self.pending_click = Some(Instant::now() + ui.double_click_interval());
        }
      }
      sdl::Event::Mouse {
        x,
        y,
        button: sdl::MIDDLE_BUTTON,
        ..
      } => {
        if self.video_view_active() {
          self.drag_position = Some(ui.render_point(x, y));
          ui.capture_mouse(true);
        }
      }
      sdl::Event::MouseUp {
        button: sdl::MIDDLE_BUTTON,
      } => {
        self.drag_position = None;
        ui.capture_mouse(false);
      }
      sdl::Event::Motion { x, y } => {
        if let Some(old) = self.drag_position {
          let next = ui.render_point(x, y);
          self.drag_position = Some(next);
          if self.video_view_active() {
            if let Some((w, h, _)) = view::fit(ui.size(), self.aspect, self.rotation()) {
              let before = self.settings.view;
              self.settings.view.pan(
                ui.size(),
                (w, h),
                (next.0.saturating_sub(old.0), next.1.saturating_sub(old.1)),
              );
              if before != self.settings.view {
                self.save_settings();
              }
            }
          }
        }
      }
      sdl::Event::Wheel { x, y, delta } => {
        if self.video_view_active() {
          if let Some((w, h, _)) = view::fit(ui.size(), self.aspect, self.rotation()) {
            let before = self.settings.view;
            self
              .settings
              .view
              .zoom_at(ui.size(), (w, h), ui.render_point(x, y), delta);
            if before != self.settings.view {
              self.save_settings();
            }
          }
        }
      }
      sdl::Event::FocusLost => {
        self.drag_position = None;
        self.pending_click = None;
        ui.capture_mouse(false);
      }
      sdl::Event::Resized => {
        if self.drag_position.is_some() {
          self.drag_position = ui.mouse();
        }
      }
      sdl::Event::Mouse {
        button: sdl::RIGHT_BUTTON,
        ..
      } => {
        self.menu = !self.menu;
      }
      _ => {}
    }
    false
  }

  fn rotation(&self) -> f64 {
    self.metadata.as_ref().map_or(0.0, |media| media.rotation)
  }

  fn video_view_active(&self) -> bool {
    self.displayed_video.is_some() && self.visualization == visualizer::Mode::Video
  }

  fn save_settings(&self) {
    if let Err(error) = self.settings.save(&self.media_path) {
      let path = crate::window_geometry::directory(&self.media_path, "toview")
        .map(|dir| dir.join("playback/settings.txt"))
        .unwrap_or_else(|| self.media_path.clone());
      eprintln!(
        "toview: {}",
        crate::i18n::format(
          "could not save {path}: {error}",
          &[
            ("path", &path.display().to_string()),
            ("error", &error.to_string())
          ]
        )
      );
    }
  }

  fn toggle_playback(&mut self) {
    if self.ended {
      self.seek(Target::Time(0));
      self.pause(false);
    } else {
      self.pause(!self.paused);
    }
  }

  fn seek_fraction(&mut self, fraction: f64) {
    if let Some(duration) = self.duration() {
      let position = (fraction * duration as f64).round() as i64;
      if position >= duration {
        self.seek(Target::Last);
      } else {
        self.seek(Target::Time(position));
      }
    }
  }

  fn display_position(&self) -> i64 {
    match self.seek_target {
      Some(Target::Time(position)) => position,
      Some(Target::First) => 0,
      Some(Target::Last) => self.duration().unwrap_or(self.position),
      Some(Target::Frame { current, .. }) => current,
      None => self
        .duration()
        .map_or(self.position, |d| self.position.min(d)),
    }
  }

  fn relative(&mut self, seconds: i64) {
    if seconds > 0 && self.ended {
      return;
    }
    let target = self.display_position().saturating_add(seconds * 1_000_000);
    if target <= 0 {
      self.seek(Target::First);
    } else if self.duration().is_some_and(|duration| target >= duration) {
      self.seek(Target::Last);
    } else {
      self.seek(Target::Time(target));
    }
  }

  fn chapter(&mut self, forward: bool) {
    let Some(metadata) = &self.metadata else {
      return;
    };
    if metadata.chapters.len() <= 1 {
      self.relative(if forward { 600 } else { -600 });
      return;
    }
    let position = self.display_position();
    let index = metadata.chapters.partition_point(|pts| *pts <= position);
    let destination = if forward {
      index
    } else {
      index.saturating_sub(2)
    };
    if let Some(&target) = metadata.chapters.get(destination) {
      self.seek(Target::Time(target));
    }
  }

  fn cycle_tracks(&mut self, audio: bool, video: bool) {
    let Some(metadata) = &self.metadata else {
      return;
    };
    let tracks = TrackChange {
      audio: audio && metadata.audio_tracks > 1,
      video: video && metadata.video_tracks > 1,
    };
    if tracks.audio || tracks.video {
      self.request_seek(Target::Time(self.display_position()), tracks);
    }
  }

  fn seek(&mut self, target: Target) {
    self.request_seek(target, TrackChange::default());
  }

  fn request_seek(&mut self, target: Target, tracks: TrackChange) {
    if self.metadata.is_none()
      || (matches!(target, Target::Last)
        && (self.ended || matches!(self.seek_target, Some(Target::Last))))
      || (matches!(target, Target::First)
        && self.paused
        && self.position == 0
        && self.seek_target.is_none())
    {
      return;
    }
    if matches!(target, Target::First | Target::Last) {
      self.pause(true);
    }
    self.generation += 1;
    self.requested.store(self.generation, Ordering::Release);
    let _ = self.commands.send(Seek {
      generation: self.generation,
      target,
      tracks,
    });
    self.seek_target = Some(target);
    self.video.clear();
    self.deferred = None;
    self.eof = false;
    self.ended = false;
    self.stepping = false;
    self.audio_tail = None;
    self.audio_drained_at = None;
    self.samples.clear();
    if let Some(audio) = &self.audio {
      audio.pause(true);
      audio.clear();
    }
  }

  fn pause(&mut self, paused: bool) {
    self.update_clock();
    self.paused = paused;
    self.clock.set(self.position);
    if let Some(audio) = &self.audio {
      audio.pause(paused || self.seek_target.is_some());
    }
  }

  fn receive(&mut self) -> Result<(), String> {
    for _ in 0..12 {
      let message = match self.deferred.take() {
        Some(message) => message,
        None => match self.events.try_recv() {
          Ok(message) => message,
          Err(TryRecvError::Empty) => break,
          Err(TryRecvError::Disconnected) if self.eof => break,
          Err(TryRecvError::Disconnected) => return Err(crate::i18n::tr("media decoder exited")),
        },
      };
      if message.generation != self.generation {
        continue;
      }
      match message.event {
        Event::Open(metadata) => {
          let use_audio = metadata.has_audio && self.audio.is_none();
          let audio_only = !metadata.has_video;
          self.metadata = Some(metadata);
          if audio_only {
            self.visualization = visualizer::Mode::Spectrum;
          }
          if use_audio {
            match audio_output::Output::open() {
              Ok(audio) => self.audio = Some(audio),
              Err(error) if audio_only => {
                return Err(crate::i18n::format(
                  "audio output unavailable: {error}",
                  &[("error", &error)],
                ));
              }
              Err(error) => eprintln!(
                "toview: {}",
                crate::i18n::format("audio output unavailable: {error}", &[("error", &error)])
              ),
            }
          }
        }
        Event::Video(frame) if self.video.len() >= VIDEO_QUEUE_FRAMES => {
          self.deferred = Some(Message {
            event: Event::Video(frame),
            ..message
          });
          break;
        }
        Event::Video(frame) => self.video.push_back(frame),
        Event::Audio(audio)
          if self.metadata.as_ref().is_some_and(|media| !media.has_video)
            && self
              .audio
              .as_ref()
              .is_some_and(|device| device.queued() > AUDIO_QUEUE_MICROS) =>
        {
          if self.seek_target.is_none() && self.started {
            self.deferred = Some(Message {
              event: Event::Audio(audio),
              ..message
            });
            break;
          }
        }
        Event::Audio(audio) => self.queue_audio(audio)?,
        Event::End => self.eof = true,
        Event::SeekFailed(error) => {
          eprintln!("toview: {error}");
          self.seek_target = None;
          self.clock.set(self.position);
          if let Some(audio) = &self.audio {
            audio.pause(self.paused);
          }
        }
        Event::Error(error) => return Err(error),
      }
    }
    Ok(())
  }

  fn queue_audio(&mut self, mut audio: media::Audio) -> Result<(), String> {
    if self.metadata.as_ref().is_some_and(|media| !media.has_video)
      && (!self.started || self.seek_target.is_some())
    {
      self.position = audio.pts.max(0);
      self.clock.set(self.position);
      self.seek_target = None;
      self.started = true;
    }
    if self.visualization != visualizer::Mode::Video && (!self.paused || self.samples.is_empty()) {
      let start = audio.samples.len().saturating_sub(512 * media::CHANNELS);
      self.samples.clear();
      self.samples.extend(
        audio.samples[start..]
          .chunks_exact(media::CHANNELS)
          .map(|chunk| chunk.iter().sum::<f32>() / media::CHANNELS as f32),
      );
      self.redraw = true;
    }
    let Some(device) = &self.audio else {
      return Ok(());
    };
    let gain = if self.settings.muted {
      0.0
    } else {
      self.settings.volume as f32 / 1000.0
    };
    if gain != 1.0 {
      for sample in &mut audio.samples {
        *sample *= gain;
      }
    }
    let duration =
      (audio.samples.len() / media::CHANNELS) as i64 * 1_000_000 / i64::from(media::SAMPLE_RATE);
    device.queue(&audio.samples)?;
    self.audio_tail = Some(audio.pts.saturating_add(duration));
    self.audio_drained_at = None;
    if self.started && !self.paused && self.seek_target.is_none() {
      device.pause(false);
    }
    Ok(())
  }

  fn update_clock(&mut self) {
    if !self.started || self.paused || self.ended || self.seek_target.is_some() {
      return;
    }
    if let (Some(audio), Some(tail)) = (&self.audio, self.audio_tail) {
      let queued = audio.queued();
      let elapsed = if queued > 0 {
        self.audio_drained_at = None;
        0
      } else {
        self
          .audio_drained_at
          .get_or_insert_with(Instant::now)
          .elapsed()
          .as_micros() as i64
      };
      let played = tail
        .saturating_sub(queued)
        .saturating_sub(audio.latency())
        .saturating_add(elapsed);
      self.position = self.position.max(played);
      self.clock.set(self.position);
    } else {
      self.position = self.clock.running();
    }
  }

  fn next_frame(&mut self) -> Option<media::Video> {
    if self.video.is_empty() {
      return None;
    }
    if !self.started || self.seek_target.is_some() || self.stepping {
      let frame = self.video.pop_front()?;
      let target = self.seek_target.take();
      self.position = match target {
        Some(Target::Time(time)) => time.max(frame.pts),
        Some(Target::Last) => self.duration().unwrap_or(frame.pts),
        _ => frame.pts,
      };
      self.clock.set(self.position);
      self.started = true;
      self.stepping = false;
      if matches!(target, Some(Target::Last)) {
        self.ended = true;
      }
      if let Some(audio) = &self.audio {
        audio.pause(self.paused);
      }
      return Some(frame);
    }
    if self.paused || self.ended {
      return None;
    }
    let mut frame = None;
    while self
      .video
      .front()
      .is_some_and(|video| video.pts <= self.position)
    {
      frame = self.video.pop_front();
    }
    frame
  }

  fn finish(&mut self, close_on_end: bool) -> Result<bool, String> {
    if !self.eof || !self.video.is_empty() || self.ended {
      return Ok(false);
    }
    if !self.started {
      return Err(crate::i18n::tr("file contains no decodable video frames"));
    }
    if let Some(target) = self.seek_target.take() {
      if matches!(target, Target::Last) {
        self.position = self.duration().unwrap_or(self.position);
        self.ended = true;
        self.redraw = true;
        return Ok(false);
      }
      self.clock.set(self.position);
    }
    if self.paused
      || self.position < self.frame_end
      || self
        .audio
        .as_ref()
        .is_some_and(|device| device.queued() > 0)
    {
      return Ok(false);
    }
    if let Some(audio) = &self.audio
      && self.audio_tail.is_some()
      && self
        .audio_drained_at
        .is_some_and(|time| time.elapsed().as_micros() < audio.latency() as u128)
    {
      return Ok(false);
    }
    self.ended = true;
    self.position = self.duration().unwrap_or(self.position);
    self.redraw = true;
    Ok(close_on_end)
  }
}
