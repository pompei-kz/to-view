# to-view

A minimal video player with a Rust playback engine. It uses FFmpeg for demuxing and decoding and SDL2 for the window, input, and output. Open a local video file to start playback immediately. Press Space to toggle pause. Esc toggles the menu overlay in the upper-left corner and preserves the current playback state. To quit, press the physical Q/Й key in any keyboard layout or use the window's close button.

The bundled FFmpeg build supports MP4/MOV, Matroska/WebM, AVI, and MPEG-TS. Common codecs include H.264, HEVC, AV1, VP8, VP9, AAC, and Opus.

The seekbar appears at the bottom when the menu is open. Hover over it to preview a timestamp and left-click to jump there.

The Left and Right arrow keys seek by 1 second. Hold Shift to seek by 5 seconds or Shift+Alt to seek by 30 seconds. At the boundaries, the arrow keys stop on the first or last frame. Home and End jump directly to the first and last frames, respectively, and pause playback.

By default, the window stays open on the last frame. Use `--close-on-end=1`, `-j`, or `-j1` to close it automatically. Use `--close-on-end=0`, `-J`, or `-j0` to keep it open.

The player restores its window position and size across launches. On exit, it saves the last normal window geometry as two space-separated integers per file in `~/.config/to-view/saves/window/position.txt` and `~/.config/to-view/saves/window/size.txt`.

## Building on Debian or Ubuntu

Install Rust with Cargo, then install the build tools and X11 headers listed below. If the static X11 archives are missing, the build script can download and extract the X11 packages into the Cargo build directory without administrator privileges.

```sh
sudo apt install build-essential binutils pkg-config curl xz-utils bzip2 clang libclang-dev \
  libx11-dev libxext-dev libxcb1-dev libxau-dev libxdmcp-dev x11proto-dev xtrans-dev
cargo build --release
./target/release/to-view /path/to/video/file.mp4
```

On the first build, Cargo downloads the pinned source releases of FFmpeg, SDL2, and ALSA. The build also uses a pinned bindgen CLI and libclang to generate Rust bindings. The resulting binary includes those libraries, the X11 client, and the icon. The player needs access to an X11 or Xwayland server to open a window and uses the desktop audio server (PipeWire’s PulseAudio interface or PulseAudio) when available. It falls back to SDL2 with ALSA. Set `AUDIODEV` to select an audio device, for example `AUDIODEV=plughw:1,0`. This version targets Linux with X11 and uses software rendering.

```sh
readelf -l -d target/release/to-view
```

The static binary has no `INTERP` program header or `NEEDED` entries. For paths that begin with a hyphen, use `to-view -- -video.mp4`.
