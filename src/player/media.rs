use super::ffi::*;
use std::{
  ffi::{CStr, CString, c_int, c_void},
  ptr::{self, NonNull},
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{Receiver, SyncSender, TrySendError},
  },
  time::Duration,
};

const NO_PTS: i64 = i64::MIN;
const TIME_BASE: AVRational = AVRational {
  num: 1,
  den: 1_000_000,
};
const EAGAIN: i32 = -11;
const EOF: i32 = -541_478_725;
pub const SAMPLE_RATE: i32 = 48_000;
pub const CHANNELS: usize = 2;
pub const STOP: u64 = u64::MAX;

#[derive(Clone, Copy)]
pub enum Target {
  Time(i64),
  First,
  Last,
}

#[derive(Clone, Copy)]
pub struct Seek {
  pub generation: u64,
  pub target: Target,
  pub tracks: TrackChange,
}

#[derive(Clone, Copy, Default)]
pub struct TrackChange {
  pub audio: bool,
  pub video: bool,
}

#[derive(Clone)]
pub struct Metadata {
  pub duration: Option<i64>,
  pub has_audio: bool,
  pub has_video: bool,
  pub audio_tracks: usize,
  pub video_tracks: usize,
  pub chapters: Arc<[i64]>,
  pub rotation: f64,
}

pub struct Video {
  pub pts: i64,
  pub duration: i64,
  pub width: i32,
  pub height: i32,
  pub aspect: f64,
  pub pixels: Vec<u8>,
}

pub struct Audio {
  pub pts: i64,
  pub samples: Vec<f32>,
}

pub enum Event {
  Open(Metadata),
  Video(Video),
  Audio(Audio),
  End,
  SeekFailed(String),
  Error(String),
}

pub struct Message {
  pub generation: u64,
  pub event: Event,
}

struct Interrupt {
  requested: Arc<AtomicU64>,
  active: u64,
}

extern "C" fn interrupt(opaque: *mut c_void) -> c_int {
  let state = unsafe { &*opaque.cast::<Interrupt>() };
  i32::from(state.requested.load(Ordering::Acquire) > state.active)
}

fn ff_error(code: i32) -> String {
  let mut buffer = [0; 128];
  unsafe {
    av_strerror(code, buffer.as_mut_ptr(), buffer.len());
    CStr::from_ptr(buffer.as_ptr())
      .to_string_lossy()
      .into_owned()
  }
}

fn check(code: i32, action: &str) -> Result<i32, String> {
  if code < 0 {
    Err(format!("{action}: {}", ff_error(code)))
  } else {
    Ok(code)
  }
}

struct Input {
  raw: NonNull<AVFormatContext>,
  interrupt: Box<Interrupt>,
}

impl Input {
  fn open(path: &CStr, requested: Arc<AtomicU64>) -> Result<Self, String> {
    let mut interrupt_state = Box::new(Interrupt {
      requested,
      active: 0,
    });
    unsafe {
      let mut raw = avformat_alloc_context();
      if raw.is_null() {
        return Err("could not allocate input context".into());
      }
      (*raw).interrupt_callback = AVIOInterruptCB {
        callback: Some(interrupt),
        opaque: (&mut *interrupt_state as *mut Interrupt).cast(),
      };
      check(
        avformat_open_input(&mut raw, path.as_ptr(), ptr::null(), ptr::null_mut()),
        "open input",
      )?;
      let input = Self {
        raw: NonNull::new(raw).unwrap(),
        interrupt: interrupt_state,
      };
      check(
        avformat_find_stream_info(raw, ptr::null_mut()),
        "read stream information",
      )?;
      Ok(input)
    }
  }

  fn stream(&self, kind: AVMediaType) -> Option<&AVStream> {
    unsafe {
      let index = av_find_best_stream(self.raw.as_ptr(), kind, -1, -1, ptr::null_mut(), 0);
      (index >= 0).then(|| &**(*self.raw.as_ptr()).streams.add(index as usize))
    }
  }

  fn streams(&self, kind: AVMediaType) -> Vec<&AVStream> {
    unsafe {
      let raw = self.raw.as_ref();
      (0..raw.nb_streams as usize)
        .map(|i| &**raw.streams.add(i))
        .filter(|s| {
          let parameters = &*s.codecpar;
          parameters.codec_type == kind && !avcodec_find_decoder(parameters.codec_id).is_null()
        })
        .collect()
    }
  }

  fn chapters(&self, start: i64) -> Arc<[i64]> {
    unsafe {
      let raw = self.raw.as_ref();
      (0..raw.nb_chapters as usize)
        .filter_map(|i| {
          let chapter = &**raw.chapters.add(i);
          time(chapter.start, chapter.time_base).map(|value| value.saturating_sub(start).max(0))
        })
        .collect()
    }
  }

  fn seek(&mut self, seek: Seek, start: i64, end_hint: Option<i64>) -> Result<bool, String> {
    self.interrupt.active = seek.generation;
    unsafe {
      let raw = self.raw.as_ptr();
      if (*(*raw).iformat).flags & AVFMT_TS_DISCONT as i32 != 0 {
        check(
          av_seek_frame(raw, -1, 0, AVSEEK_FLAG_BYTE as i32),
          "seek input",
        )?;
        return Ok(false);
      }

      if matches!(seek.target, Target::Last)
        && let Some(timestamp) = end_hint.filter(|timestamp| *timestamp > start)
        && avformat_seek_file(raw, -1, i64::MIN, timestamp, timestamp, 0) >= 0
      {
        return Ok(true);
      }
      let timestamp = match seek.target {
        Target::Time(position) => start.saturating_add(position),
        Target::First | Target::Last => start,
      };
      check(
        avformat_seek_file(raw, -1, i64::MIN, timestamp, timestamp, 0),
        "seek input",
      )?;
    }
    Ok(false)
  }
}

impl Drop for Input {
  fn drop(&mut self) {
    unsafe { avformat_close_input(&mut self.raw.as_ptr()) };
  }
}

struct Packet(NonNull<AVPacket>);

impl Packet {
  fn new() -> Result<Self, String> {
    NonNull::new(unsafe { av_packet_alloc() })
      .map(Self)
      .ok_or_else(|| "could not allocate packet".into())
  }
}

impl Drop for Packet {
  fn drop(&mut self) {
    unsafe { av_packet_free(&mut self.0.as_ptr()) };
  }
}

struct Frame(NonNull<AVFrame>);

impl Frame {
  fn new() -> Result<Self, String> {
    NonNull::new(unsafe { av_frame_alloc() })
      .map(Self)
      .ok_or_else(|| "could not allocate frame".into())
  }
}

impl Drop for Frame {
  fn drop(&mut self) {
    unsafe { av_frame_free(&mut self.0.as_ptr()) };
  }
}

struct Decoder {
  raw: NonNull<AVCodecContext>,
  index: i32,
  time_base: AVRational,
  frame: Frame,
}

impl Decoder {
  fn open(stream: &AVStream) -> Result<Self, String> {
    let frame = Frame::new()?;
    unsafe {
      let codec = avcodec_find_decoder((*stream.codecpar).codec_id);
      if codec.is_null() {
        return Err("no decoder for selected stream".into());
      }
      let raw = NonNull::new(avcodec_alloc_context3(codec)).ok_or("could not allocate decoder")?;
      let decoder = Self {
        raw,
        index: stream.index,
        time_base: stream.time_base,
        frame,
      };
      check(
        avcodec_parameters_to_context(raw.as_ptr(), stream.codecpar),
        "configure decoder",
      )?;
      (*raw.as_ptr()).pkt_timebase = stream.time_base;
      check(
        avcodec_open2(raw.as_ptr(), codec, ptr::null_mut()),
        "open decoder",
      )?;
      Ok(decoder)
    }
  }

  fn flush(&mut self) {
    unsafe { avcodec_flush_buffers(self.raw.as_ptr()) };
  }

  fn decode(
    &mut self,
    packet: *const AVPacket,
    mut receive: impl FnMut(&AVFrame, AVRational) -> Result<(), String>,
  ) -> Result<(), String> {
    unsafe {
      let code = avcodec_send_packet(self.raw.as_ptr(), packet);
      if code == EOF && packet.is_null() {
        return Ok(());
      }
      check(code, "send packet")?;
      loop {
        let code = avcodec_receive_frame(self.raw.as_ptr(), self.frame.0.as_ptr());
        if code == EAGAIN || code == EOF {
          return Ok(());
        }
        check(code, "decode frame")?;
        let result = receive(self.frame.0.as_ref(), self.time_base);
        av_frame_unref(self.frame.0.as_ptr());
        result?;
      }
    }
  }
}

impl Drop for Decoder {
  fn drop(&mut self) {
    unsafe { avcodec_free_context(&mut self.raw.as_ptr()) };
  }
}

fn time(value: i64, base: AVRational) -> Option<i64> {
  (value != NO_PTS).then(|| unsafe { av_rescale_q(value, base, TIME_BASE) })
}

#[derive(Default)]
struct ConvertVideo {
  raw: *mut SwsContext,
}

impl ConvertVideo {
  fn frame(&mut self, frame: &AVFrame, pts: i64, duration: i64) -> Result<Video, String> {
    let (width, height) = (frame.width, frame.height);
    if !(1..=8192).contains(&width) || !(1..=8192).contains(&height) {
      return Err(format!("unsupported video size {width}x{height}"));
    }
    let mut pixels = vec![0; width as usize * height as usize * 4 + 64];
    let ratio = frame.sample_aspect_ratio;
    let aspect = width as f64 / height as f64
      * if ratio.num > 0 && ratio.den > 0 {
        ratio.num as f64 / ratio.den as f64
      } else {
        1.0
      };
    unsafe {
      self.raw = sws_getCachedContext(
        self.raw,
        width,
        height,
        frame.format as AVPixelFormat,
        width,
        height,
        AVPixelFormat_AV_PIX_FMT_RGBA,
        SWS_BILINEAR as i32,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null(),
      );
      if self.raw.is_null() {
        return Err("could not configure video conversion".into());
      }
      let output = [
        pixels.as_mut_ptr(),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
      ];
      check(
        sws_scale(
          self.raw,
          frame.data.as_ptr().cast(),
          frame.linesize.as_ptr(),
          0,
          height,
          output.as_ptr(),
          [width * 4, 0, 0, 0].as_ptr(),
        ),
        "convert video frame",
      )?;
    }
    Ok(Video {
      pts,
      duration,
      width,
      height,
      aspect,
      pixels,
    })
  }
}

impl Drop for ConvertVideo {
  fn drop(&mut self) {
    unsafe { sws_freeContext(self.raw) };
  }
}

#[derive(Default)]
struct ConvertAudio {
  raw: *mut SwrContext,
  layout: Option<AVChannelLayout>,
  rate: i32,
  format: i32,
  next_pts: Option<i64>,
}

impl ConvertAudio {
  fn configure(&mut self, frame: &AVFrame) -> Result<(), String> {
    unsafe {
      if self.rate == frame.sample_rate
        && self.format == frame.format
        && self
          .layout
          .as_ref()
          .is_some_and(|layout| av_channel_layout_compare(layout, &frame.ch_layout) == 0)
      {
        return Ok(());
      }
      self.reset();
      let mut layout: AVChannelLayout = std::mem::zeroed();
      check(
        av_channel_layout_copy(&mut layout, &frame.ch_layout),
        "copy channel layout",
      )?;
      self.layout = Some(layout);
      let mut out: AVChannelLayout = std::mem::zeroed();
      av_channel_layout_default(&mut out, CHANNELS as i32);
      let result = swr_alloc_set_opts2(
        &mut self.raw,
        &out,
        AVSampleFormat_AV_SAMPLE_FMT_FLT,
        SAMPLE_RATE,
        self.layout.as_ref().unwrap(),
        frame.format as AVSampleFormat,
        frame.sample_rate,
        0,
        ptr::null_mut(),
      );
      av_channel_layout_uninit(&mut out);
      check(result, "configure audio conversion")?;
      check(swr_init(self.raw), "initialize audio conversion")?;
      self.rate = frame.sample_rate;
      self.format = frame.format;
    }
    Ok(())
  }

  fn samples(&mut self, frame: Option<&AVFrame>, pts: i64) -> Result<Audio, String> {
    if let Some(frame) = frame {
      self.configure(frame)?;
    }
    if self.raw.is_null() {
      return Ok(Audio {
        pts,
        samples: Vec::new(),
      });
    }
    unsafe {
      let count = frame.map_or(0, |f| f.nb_samples);
      let capacity = check(swr_get_out_samples(self.raw, count), "size audio buffer")?;
      let mut samples = vec![0.0_f32; capacity as usize * CHANNELS];
      let output = [samples.as_mut_ptr().cast::<u8>()];
      let input = frame.map_or(ptr::null_mut(), |f| f.extended_data.cast::<*const u8>());
      let converted = check(
        swr_convert(self.raw, output.as_ptr().cast_mut(), capacity, input, count),
        "convert audio samples",
      )?;
      samples.truncate(converted as usize * CHANNELS);
      let pts = self
        .next_pts
        .filter(|next| pts.abs_diff(*next) < 100_000 || frame.is_none())
        .unwrap_or(pts);
      self.next_pts =
        Some(pts.saturating_add(i64::from(converted) * 1_000_000 / i64::from(SAMPLE_RATE)));
      Ok(Audio { pts, samples })
    }
  }

  fn reset(&mut self) {
    unsafe {
      swr_free(&mut self.raw);
      if let Some(mut layout) = self.layout.take() {
        av_channel_layout_uninit(&mut layout);
      }
    }
    self.next_pts = None;
  }
}

impl Drop for ConvertAudio {
  fn drop(&mut self) {
    self.reset();
  }
}

struct Publisher {
  tx: SyncSender<Message>,
  requested: Arc<AtomicU64>,
  generation: u64,
}

impl Publisher {
  fn stale(&self) -> bool {
    self.requested.load(Ordering::Acquire) > self.generation
  }

  fn send(&self, event: Event) -> bool {
    let mut message = Message {
      generation: self.generation,
      event,
    };
    while !self.stale() {
      match self.tx.try_send(message) {
        Ok(()) => return true,
        Err(TrySendError::Full(unsent)) => {
          message = unsent;
          std::thread::sleep(Duration::from_millis(2));
        }
        Err(TrySendError::Disconnected(_)) => break,
      }
    }
    false
  }
}

struct Output {
  video: ConvertVideo,
  audio: ConvertAudio,
  start: i64,
  target: Target,
  last_video_pts: Option<i64>,
  video_interval: i64,
  next_video: i64,
  next_audio: i64,
}

impl Output {
  fn new(start: i64, video_interval: i64) -> Self {
    Self {
      video: ConvertVideo::default(),
      audio: ConvertAudio::default(),
      start,
      target: Target::Time(0),
      last_video_pts: None,
      video_interval,
      next_video: 0,
      next_audio: 0,
    }
  }

  fn reset(&mut self, target: Target) {
    self.audio.reset();
    self.target = target;
    self.last_video_pts = None;
    self.next_video = 0;
    self.next_audio = 0;
  }

  fn video(
    &mut self,
    frame: &AVFrame,
    base: AVRational,
    publisher: &Publisher,
  ) -> Result<(), String> {
    if publisher.stale() {
      return Ok(());
    }
    let pts = time(frame.best_effort_timestamp, base)
      .map_or(self.next_video, |pts| pts.saturating_sub(self.start));
    let duration = time(frame.duration, base)
      .filter(|d| *d > 0)
      .unwrap_or(self.video_interval);
    self.next_video = pts.saturating_add(duration);
    if matches!(self.target, Target::Last) {
      self.last_video_pts = Some(pts);
      // Decode through EOF and only convert the last frame. Retain the decoded reference.
      return Ok(());
    }
    if let Target::Time(target) = self.target {
      if pts < target && self.next_video <= target {
        return Ok(());
      }
    }
    let video = self.video.frame(frame, pts.max(0), duration)?;
    publisher.send(Event::Video(video));
    Ok(())
  }

  fn audio(
    &mut self,
    frame: &AVFrame,
    base: AVRational,
    publisher: &Publisher,
  ) -> Result<(), String> {
    if publisher.stale() || matches!(self.target, Target::Last) {
      return Ok(());
    }
    let pts = time(frame.pts, base).map_or(self.next_audio, |pts| pts.saturating_sub(self.start));
    self.next_audio = pts.saturating_add(
      i64::from(frame.nb_samples) * 1_000_000 / i64::from(frame.sample_rate.max(1)),
    );
    let mut audio = self.audio.samples(Some(frame), pts)?;
    if let Target::Time(target) = self.target {
      let skip = ((target.saturating_sub(audio.pts).max(0) as i128 * SAMPLE_RATE as i128 + 999_999)
        / 1_000_000)
        .min((audio.samples.len() / CHANNELS) as i128) as usize;
      audio.samples.drain(..skip * CHANNELS);
      audio.pts += skip as i64 * 1_000_000 / i64::from(SAMPLE_RATE);
    }
    if !audio.samples.is_empty() {
      publisher.send(Event::Audio(audio));
    }
    Ok(())
  }
}

struct Media {
  input: Input,
  video: Option<Decoder>,
  audio: Option<Decoder>,
  packet: Packet,
  last: Frame,
  output: Output,
  metadata: Metadata,
  last_from_hint: bool,
}

impl Media {
  fn open(path: &CStr, requested: Arc<AtomicU64>) -> Result<Self, String> {
    let input = Input::open(path, requested)?;
    let video_stream = input.stream(AVMediaType_AVMEDIA_TYPE_VIDEO);
    let video = video_stream.map(Decoder::open).transpose()?;
    let audio =
      input
        .stream(AVMediaType_AVMEDIA_TYPE_AUDIO)
        .and_then(|stream| match Decoder::open(stream) {
          Ok(audio) => Some(audio),
          Err(error) => {
            eprintln!("to-view: {error}");
            None
          }
        });
    if video.is_none() && audio.is_none() {
      return Err("no playable audio or video stream".into());
    }
    let start = unsafe { (*input.raw.as_ptr()).start_time };
    let start = if start == NO_PTS { 0 } else { start };
    let duration = unsafe { (*input.raw.as_ptr()).duration };
    let (interval, rotation) = video_stream.map_or((40_000, 0.0), video_settings);
    let metadata = Metadata {
      duration: (duration > 0).then_some(duration),
      has_audio: audio.is_some(),
      has_video: video.is_some(),
      audio_tracks: input.streams(AVMediaType_AVMEDIA_TYPE_AUDIO).len(),
      video_tracks: input.streams(AVMediaType_AVMEDIA_TYPE_VIDEO).len(),
      chapters: input.chapters(start),
      rotation,
    };
    Ok(Self {
      input,
      video,
      audio,
      packet: Packet::new()?,
      last: Frame::new()?,
      output: Output::new(start, interval),
      metadata,
      last_from_hint: false,
    })
  }

  fn select_tracks(&mut self, tracks: TrackChange) -> Result<(), String> {
    if tracks.video {
      let streams = self.input.streams(AVMediaType_AVMEDIA_TYPE_VIDEO);
      if let Some(stream) = next_track(
        &streams,
        self.video.as_ref().map_or(-1, |video| video.index),
      ) {
        let decoder = Decoder::open(stream)?;
        let (interval, rotation) = video_settings(stream);
        self.video = Some(decoder);
        self.metadata.has_video = true;
        self.output.video_interval = interval;
        self.metadata.rotation = rotation;
      }
    }
    if tracks.audio {
      let streams = self.input.streams(AVMediaType_AVMEDIA_TYPE_AUDIO);
      if let Some(stream) = next_track(
        &streams,
        self.audio.as_ref().map_or(-1, |audio| audio.index),
      ) {
        self.audio = Some(Decoder::open(stream)?);
        self.metadata.has_audio = true;
      }
    }
    Ok(())
  }

  fn seek(&mut self, seek: Seek, allow_hint: bool) -> Result<(), String> {
    let end_hint = if allow_hint && matches!(seek.target, Target::Last) {
      self.video.as_ref().and_then(|decoder| {
        let stream = unsafe { &**self.input.raw.as_ref().streams.add(decoder.index as usize) };
        let start = time(stream.start_time, stream.time_base).unwrap_or(self.output.start);
        let duration = time(stream.duration, stream.time_base).filter(|v| *v > 0)?;
        Some(start.saturating_add(duration).saturating_sub(1))
      })
    } else {
      None
    };
    self.last_from_hint = self.input.seek(seek, self.output.start, end_hint)?;
    if let Some(video) = &mut self.video {
      video.flush();
    }
    if let Some(audio) = &mut self.audio {
      audio.flush();
    }
    unsafe { av_frame_unref(self.last.0.as_ptr()) };
    self.output.reset(seek.target);
    Ok(())
  }

  fn decode_video(&mut self, packet: *const AVPacket, publisher: &Publisher) -> Result<(), String> {
    let last = self.last.0.as_ptr();
    let output = &mut self.output;
    let Some(video) = &mut self.video else {
      return Ok(());
    };
    video.decode(packet, |frame, base| {
      output.video(frame, base, publisher)?;
      if matches!(output.target, Target::Last) && !publisher.stale() {
        unsafe {
          av_frame_unref(last);
          check(av_frame_ref(last, frame), "retain final video frame")?;
        }
      }
      Ok(())
    })
  }

  fn pump(&mut self, publisher: &Publisher) -> Result<(), String> {
    while !publisher.stale() {
      let packet = self.packet.0.as_ptr();
      let result = unsafe { av_read_frame(self.input.raw.as_ptr(), packet) };
      if result == EOF {
        self.decode_video(ptr::null(), publisher)?;
        if self.last_from_hint && self.output.last_video_pts.is_none() && !publisher.stale() {
          self.seek(
            Seek {
              generation: publisher.generation,
              target: Target::Last,
              tracks: TrackChange::default(),
            },
            false,
          )?;
          continue;
        }
        if matches!(self.output.target, Target::Last) {
          if let Some(pts) = self.output.last_video_pts {
            let video = self.output.video.frame(
              unsafe { self.last.0.as_ref() },
              pts.max(0),
              self.output.next_video.saturating_sub(pts),
            )?;
            publisher.send(Event::Video(video));
          }
        } else if let Some(audio) = &mut self.audio {
          audio.decode(ptr::null(), |frame, base| {
            self.output.audio(frame, base, publisher)
          })?;
          let data = self.output.audio.samples(None, self.output.next_audio)?;
          if !data.samples.is_empty() {
            publisher.send(Event::Audio(data));
          }
        }
        publisher.send(Event::End);
        return Ok(());
      }
      if publisher.stale() {
        unsafe { av_packet_unref(packet) };
        break;
      }
      check(result, "read media packet")?;
      let index = unsafe { (*packet).stream_index };
      let decoded = if self
        .video
        .as_ref()
        .is_some_and(|video| index == video.index)
      {
        self.decode_video(packet, publisher)
      } else if let Some(audio) = self.audio.as_mut().filter(|audio| audio.index == index) {
        if matches!(self.output.target, Target::Last) {
          Ok(())
        } else {
          audio.decode(packet, |frame, base| {
            self.output.audio(frame, base, publisher)
          })
        }
      } else {
        Ok(())
      };
      unsafe { av_packet_unref(packet) };
      decoded?;
    }
    Ok(())
  }
}

fn next_track<'a>(streams: &[&'a AVStream], current: i32) -> Option<&'a AVStream> {
  if streams.len() < 2 {
    return None;
  }
  let index = streams.iter().position(|s| s.index == current);
  Some(streams[index.map_or(0, |i| (i + 1) % streams.len())])
}

fn video_settings(stream: &AVStream) -> (i64, f64) {
  let rate = stream.avg_frame_rate;
  let interval = if rate.num > 0 && rate.den > 0 {
    (1_000_000 * i64::from(rate.den) / i64::from(rate.num)).max(1)
  } else {
    40_000
  };
  let rotation = unsafe {
    let codec = &*stream.codecpar;
    let matrix = av_packet_side_data_get(
      codec.coded_side_data,
      codec.nb_coded_side_data as _,
      AVPacketSideDataType_AV_PKT_DATA_DISPLAYMATRIX,
    );
    if matrix.is_null() || (*matrix).size < 36 {
      0.0
    } else {
      -av_display_rotation_get((*matrix).data.cast())
    }
  };
  (interval, if rotation.is_finite() { rotation } else { 0.0 })
}

pub fn run(
  path: CString,
  requested: Arc<AtomicU64>,
  control: Receiver<Seek>,
  tx: SyncSender<Message>,
) {
  unsafe { av_log_set_level(AV_LOG_ERROR as i32) };
  let mut publisher = Publisher {
    tx,
    requested: Arc::clone(&requested),
    generation: 0,
  };
  let mut media = match Media::open(&path, requested) {
    Ok(media) => media,
    Err(error) => {
      publisher.send(Event::Error(error));
      return;
    }
  };
  if !publisher.send(Event::Open(media.metadata.clone())) {
    return;
  }
  loop {
    if let Err(error) = media.pump(&publisher) {
      publisher.send(Event::Error(error));
      if !publisher.stale() {
        return;
      }
    }
    let Ok(mut seek) = control.recv() else { return };
    while let Ok(newer) = control.try_recv() {
      seek = newer;
    }
    publisher.generation = seek.generation;
    if let Err(error) = media
      .seek(seek, true)
      .and_then(|()| media.select_tracks(seek.tracks))
    {
      publisher.send(Event::SeekFailed(error));
    }
    if seek.tracks.audio || seek.tracks.video {
      publisher.send(Event::Open(media.metadata.clone()));
    }
  }
}
