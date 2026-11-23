# toview

A minimal video player with a Rust playback engine. It uses FFmpeg for demuxing and decoding and SDL2 for the window, input, and output. Open a local video file to start playback immediately. Press Space to toggle pause. Esc or the physical V/М key toggles the menu overlay in the upper-left corner and preserves the current playback state. Press the physical F/А key to toggle fullscreen, regardless of keyboard layout or Caps Lock. To quit, press the physical Q/Й key in any keyboard layout or use the window's close button.

The bundled FFmpeg build supports MP4/MOV, Matroska/WebM, AVI, and MPEG-TS. Common codecs include H.264, HEVC, AV1, VP8, VP9, AAC, and Opus.

Right-click to toggle the menu. Left-click the video or the menu button to toggle playback. The seekbar appears at the bottom when the menu is open. Hover over it to preview a timestamp and left-click to jump there.

While paused, Ctrl+Left and Ctrl+Right move one video frame backward or forward. The Left and Right arrow keys seek by 1 second. Hold Shift to seek by 5 seconds or Shift+Alt to seek by 30 seconds. At the boundaries, the arrow keys stop on the first or last frame. Home and End jump directly to the first and last frames, respectively, and pause playback.

In menu mode, click the three-line icon at the upper right or press the physical S/Ы key to toggle the extended menu. The controls work even when the menu is hidden, independently of keyboard layout and Caps Lock: 1/2 adjust brightness, 3/4 contrast, 5/6 gamma, and 7/8 saturation. Values start at zero, change one step at a time, and range from -100 to 100. Physical X/Ч and C/С decrease and increase volume in 2.5% steps; Z/Я toggles mute. Keyboard volume and mute buttons work when delivered by SDL. Ctrl+C cycles audio and video tracks together; A cycles audio tracks.

Picture and audio settings are saved immediately per canonical file path in `~/.config/toview/saves/<sha256>/playback/settings.txt` and restored on the next opening. Picture controls operate on video pixels only: brightness adds value/100, contrast multiplies distance from 0.5 by 2^(value/100), gamma applies the exponent 2^(-value/100), and saturation multiplies distance from luminance by 2^(value/100). Channels are clamped to the displayable range; zero preserves the original frame exactly.

By default, the window stays open on the last frame. Use `--close-on-end=1`, `-j`, or `-j1` to close it automatically. Use `--close-on-end=0`, `-J`, or `-j0` to keep it open.

The player restores its window position and size across launches. After the window stops moving or resizing for 300 ms, it saves the normal window geometry as two space-separated integers per file in `~/.config/toview/saves/<sha256>/window/position.txt` and `size.txt`. The hash is SHA-256 of the canonical absolute file path. New files open centered on the monitor containing the pointer, fitted within three quarters of its usable width and height while preserving the video aspect ratio.

## Building on Debian or Ubuntu

Install Rust with Cargo, then install the build tools and X11 headers listed below. If the static X11 archives are missing, the build script can download and extract the X11 packages into the Cargo build directory without administrator privileges.

```sh
sudo apt install build-essential binutils pkg-config curl xz-utils bzip2 clang libclang-dev gettext \
  libx11-dev libxext-dev libxcb1-dev libxau-dev libxdmcp-dev x11proto-dev xtrans-dev
cargo build --release
./target/release/toview /path/to/video/file.mp4
```

On the first build, Cargo downloads the pinned source releases of FFmpeg, SDL2, and ALSA. The build also uses a pinned bindgen CLI and libclang to generate Rust bindings. The resulting binary includes those libraries, the X11 client, and the icon. The player needs access to an X11 or Xwayland server to open a window and uses the desktop audio server (PipeWire’s PulseAudio interface or PulseAudio) when available. It falls back to SDL2 with ALSA. Set `AUDIODEV` to select an audio device, for example `AUDIODEV=plughw:1,0`. This version targets Linux with X11 and uses software rendering.

```sh
readelf -l -d target/release/toview
```

The static binary has no `INTERP` program header or `NEEDED` entries. For paths that begin with a hyphen, use `toview -- -video.mp4`.

## Translations

The UI language is selected from `LC_ALL`, `LC_MESSAGES`, `LANG`, and the colon-separated `LANGUAGES` list, in that order. A locale must be installed and have a translation. If no candidate succeeds, the player uses Russian. Russian, English, and Kazakh catalogs are embedded in the executable. Short language names in `LANGUAGES` can select an installed catalog without a matching system locale.

For other languages, install a GNU gettext catalog as `/usr/local/share/locale/<locale>/LC_MESSAGES/toview.mo` or `/usr/share/locale/<locale>/LC_MESSAGES/toview.mo`. Regional catalogs are tried before the base language. Add translations by creating a UTF-8 `.po` file based on `locales/en/LC_MESSAGES/toview.po`, then compile it with `msgfmt --check --check-format -o toview.mo toview.po`. The `gettext` build package provides `msgfmt`; it is not required to run the player.
