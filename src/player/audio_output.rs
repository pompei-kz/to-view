use super::{media, sdl};
use futures::executor::block_on;
use pulseaudio::{Client, PlaybackSource, PlaybackStream, protocol};
use std::{
  cell::{Cell, RefCell},
  collections::VecDeque,
  env, fs,
  os::unix::net::UnixStream,
  pin::Pin,
  sync::{Arc, Mutex},
  task::{Context, Poll, Waker},
  time::{Duration, Instant},
};

const BYTES_PER_FRAME: usize = media::CHANNELS * size_of::<f32>();
const BYTES_PER_SECOND: i64 = media::SAMPLE_RATE as i64 * BYTES_PER_FRAME as i64;

fn micros(bytes: i64) -> i64 {
  bytes.saturating_mul(1_000_000) / BYTES_PER_SECOND
}

pub enum Output {
  Server(Pulse),
  Sdl(sdl::AudioDevice),
}

impl Output {
  pub fn open() -> Result<Self, String> {
    let explicit_sdl = ["SDL_AUDIODRIVER", "AUDIODEV", "ALSA_CONFIG_PATH"]
      .iter()
      .any(|name| env::var_os(name).is_some_and(|value| !value.is_empty()));
    if !explicit_sdl && pulseaudio::socket_path_from_env().is_some_and(|path| path.exists()) {
      match Pulse::open() {
        Ok(output) => return Ok(Self::Server(output)),
        Err(error) => eprintln!("to-view: audio server unavailable: {error}"),
      }
    }
    sdl::AudioDevice::open().map(Self::Sdl)
  }

  pub fn pause(&self, paused: bool) {
    match self {
      Self::Server(pulse) => pulse.pause(paused),
      Self::Sdl(sdl) => sdl.pause(paused),
    }
  }

  pub fn clear(&self) {
    match self {
      Self::Server(pulse) => pulse.clear(),
      Self::Sdl(sdl) => sdl.clear(),
    }
  }

  pub fn queued(&self) -> i64 {
    match self {
      Self::Server(pulse) => pulse.queued(),
      Self::Sdl(sdl) => sdl.queued(),
    }
  }

  pub fn latency(&self) -> i64 {
    match self {
      Self::Server(pulse) => pulse.timing.borrow().as_ref().map_or(0, |t| t.sink),
      Self::Sdl(sdl) => sdl.latency,
    }
  }

  pub fn queue(&self, samples: &[f32]) -> Result<(), String> {
    match self {
      Self::Server(pulse) => {
        pulse.queue(samples);
        Ok(())
      }
      Self::Sdl(sdl) => sdl.queue(samples),
    }
  }
}

#[derive(Default)]
struct Queue {
  bytes: VecDeque<u8>,
  sent: i64,
  waker: Option<Waker>,
}

struct Source(Arc<Mutex<Queue>>);

impl PlaybackSource for Source {
  fn poll_read(self: Pin<&mut Self>, context: &mut Context<'_>, buffer: &mut [u8]) -> Poll<usize> {
    let mut queue = self.0.lock().unwrap();
    let count = queue.bytes.len().min(buffer.len()) / BYTES_PER_FRAME * BYTES_PER_FRAME;
    if count == 0 {
      queue.waker = Some(context.waker().clone());
      return Poll::Pending;
    }
    for (destination, byte) in buffer[..count].iter_mut().zip(queue.bytes.drain(..count)) {
      *destination = byte;
    }
    queue.sent += count as i64;
    Poll::Ready(count)
  }
}

struct Timing {
  sampled: Instant,
  sent: i64,
  queued: i64,
  playing: bool,
  sink: i64,
}

pub struct Pulse {
  stream: PlaybackStream,
  _client: Client,
  queue: Arc<Mutex<Queue>>,
  paused: Cell<bool>,
  timing: RefCell<Option<Timing>>,
  failed: Cell<bool>,
}

impl Pulse {
  fn open() -> Result<Self, String> {
    let path = pulseaudio::socket_path_from_env().ok_or("no local audio socket")?;
    let socket = UnixStream::connect(path).map_err(|error| error.to_string())?;
    socket
      .set_read_timeout(Some(Duration::from_secs(1)))
      .map_err(|error| error.to_string())?;
    socket
      .set_write_timeout(Some(Duration::from_secs(1)))
      .map_err(|error| error.to_string())?;
    let cookie = pulseaudio::cookie_path_from_env()
      .and_then(|path| fs::read(path).ok())
      .filter(|bytes| bytes.len() == 256)
      .unwrap_or_else(|| vec![0; 256]);
    let client = Client::new_unix(c"to-view", socket, Some(cookie))
      .map_err(|error| format!("connect to PulseAudio: {error:?}"))?;
    let queue = Arc::new(Mutex::new(Queue::default()));
    let mut params = protocol::PlaybackStreamParams {
      sample_spec: protocol::SampleSpec {
        format: protocol::SampleFormat::Float32Le,
        channels: media::CHANNELS as u8,
        sample_rate: media::SAMPLE_RATE as u32,
      },
      channel_map: protocol::ChannelMap::stereo(),
      ..Default::default()
    };
    params.flags.start_corked = true;
    params.flags.adjust_latency = true;
    params.buffer_attr.target_length = (BYTES_PER_SECOND / 20) as u32;
    params.buffer_attr.minimum_request_length = (BYTES_PER_SECOND / 100) as u32;
    params.buffer_attr.pre_buffering = 0;
    let stream = block_on(client.create_playback_stream(params, Source(Arc::clone(&queue))))
      .map_err(|error| format!("open PulseAudio stream: {error:?}"))?;
    Ok(Self {
      stream,
      _client: client,
      queue,
      paused: Cell::new(true),
      timing: RefCell::new(None),
      failed: Cell::new(false),
    })
  }

  fn pause(&self, paused: bool) {
    if self.failed.get() || self.paused.get() == paused {
      return;
    }
    let result = if paused {
      block_on(self.stream.cork())
    } else {
      block_on(self.stream.uncork())
    };
    match result {
      Ok(()) => {
        self.paused.set(paused);
        self.timing.replace(None);
      }
      Err(error) => self.fail(error),
    }
  }

  fn clear(&self) {
    self.queue.lock().unwrap().bytes.clear();
    self.timing.replace(None);
    if !self.failed.get()
      && let Err(error) = block_on(self.stream.flush())
    {
      self.fail(error);
    }
  }

  fn queue(&self, samples: &[f32]) {
    if self.failed.get() {
      return;
    }
    let waker = {
      let mut queue = self.queue.lock().unwrap();
      queue
        .bytes
        .extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
      queue.waker.take()
    };
    if let Some(waker) = waker {
      waker.wake();
    }
  }

  fn queued(&self) -> i64 {
    if self.failed.get() {
      return 0;
    }
    let mut timing = self.timing.borrow_mut();
    if timing
      .as_ref()
      .is_none_or(|t| t.sampled.elapsed() >= Duration::from_millis(30))
    {
      let sent = self.queue.lock().unwrap().sent;
      match block_on(self.stream.timing_info()) {
        Ok(info) => {
          *timing = Some(Timing {
            sampled: Instant::now(),
            sent,
            queued: info.write_offset.saturating_sub(info.read_offset).max(0),
            playing: info.playing,
            sink: info.sink_usec.min(i64::MAX as u64) as i64,
          })
        }
        Err(error) => {
          self.fail(error);
          return 0;
        }
      }
    }
    let queue = self.queue.lock().unwrap();
    let timing = timing.as_ref().unwrap();
    let elapsed = if timing.playing && !self.paused.get() {
      timing.sampled.elapsed().as_micros() as i64
    } else {
      0
    };
    micros(queue.bytes.len() as i64)
      + (micros(timing.queued + queue.sent - timing.sent) - elapsed).max(0)
  }

  fn fail(&self, error: impl std::fmt::Display) {
    if !self.failed.replace(true) {
      eprintln!("to-view: audio server disconnected: {error}");
      self.queue.lock().unwrap().bytes.clear();
    }
  }
}
