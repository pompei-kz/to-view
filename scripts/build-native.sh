#!/usr/bin/env bash
set -euo pipefail

out=$1
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
prefix="$out/install"
jobs=${TO_VIEW_BUILD_JOBS:-$(nproc)}
jobs=$((jobs > 8 ? 8 : jobs))
machine=$(cc -dumpmachine)
xroot=${TO_VIEW_X11_SYSROOT:-}
xlib="$xroot/usr/lib/$machine"
mkdir -p "$out" "$prefix"
cd "$out"

fetch() {
    local name=$1 url=$2 checksum=$3
    if [[ ! -f "$name" ]]; then
        curl --fail --location --retry 2 --max-time 180 "$url" -o "$name.part"
        mv "$name.part" "$name"
    fi
    printf '%s  %s\n' "$checksum" "$name" | sha256sum --check
}

# Use the system's static X11 packages when available. On Debian and Ubuntu
# they can also be extracted into the build directory without root access.
if [[ -z "$xroot" && ! -f "$xlib/libX11.a" ]] && command -v apt-get >/dev/null; then
    xroot="$out/x11"
    xlib="$xroot/usr/lib/$machine"
    if [[ ! -f "$xroot/.installed" ]]; then
        mkdir -p "$xroot" "$out/x11-packages"
        (
            cd "$out/x11-packages"
            apt-get download libx11-dev libxext-dev libxcb1-dev libxau-dev libxdmcp-dev x11proto-dev xtrans-dev
            for package in *.deb; do
                dpkg-deb -x "$package" "$xroot"
            done
        )
        touch "$xroot/.installed"
    fi
fi

for name in X11 Xext xcb Xau Xdmcp; do
    if [[ ! -f "$xlib/lib$name.a" ]]; then
        echo "Missing $xlib/lib$name.a; install the X11 build dependencies from README.md." >&2
        exit 1
    fi
done

printf '%s' "$xlib" > "$out/xlib-path"

fetch alsa-lib-1.2.13.tar.bz2 https://www.alsa-project.org/files/pub/lib/alsa-lib-1.2.13.tar.bz2 \
    8c4ff37553cbe89618e187e4c779f71a9bb2a8b27b91f87ed40987cc9233d8f6
fetch SDL2-2.32.10.tar.gz https://www.libsdl.org/release/SDL2-2.32.10.tar.gz \
    5f5993c530f084535c65a6879e9b26ad441169b3e25d789d83287040a9ca5165
fetch ffmpeg-7.1.1.tar.xz https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz \
    733984395e0dbbe5c046abda2dc49a5544e7e0e1e2366bba849222ae9e3a03b1

if [[ ! -f "$prefix/lib/libasound.a" ]]; then
    tar xf alsa-lib-1.2.13.tar.bz2
    (
        cd alsa-lib-1.2.13
        ./configure --prefix="$prefix" --enable-static --disable-shared \
            --disable-python --without-versioned --without-libdl --with-configdir=/usr/share/alsa
        make -j"$jobs"
        make install DESTDIR="$out/alsa-stage"
        cp -a "$out/alsa-stage$prefix/." "$prefix/"
    )
fi

if [[ ! -f "$prefix/lib/libSDL2.a" ]]; then
    tar xf SDL2-2.32.10.tar.gz
    (
        cd SDL2-2.32.10
        # Force SDL to use the code linked into this executable.
        sed -i 's/^#define SDL_DYNAMIC_API 1$/#define SDL_DYNAMIC_API 0/' src/dynapi/SDL_dynapi.h
        CPPFLAGS="-I$prefix/include ${xroot:+-I$xroot/usr/include}" \
        LDFLAGS="-static -L$prefix/lib -L$xlib" LIBS="-lxcb -lXau -lXdmcp -lm" \
        ./configure --prefix="$prefix" --enable-static --disable-shared \
            --enable-video-x11 --disable-x11-shared --disable-video-wayland \
            --disable-video-kmsdrm --disable-video-opengl --disable-video-opengles \
            --disable-video-vulkan --disable-video-x11-xcursor --disable-video-x11-xinput \
            --disable-video-x11-xfixes --disable-video-x11-xrandr --disable-video-x11-scrnsaver \
            --enable-alsa --disable-alsa-shared --with-alsa-prefix="$prefix/lib" \
            --with-alsa-inc-prefix="$prefix/include" --disable-pulseaudio --disable-pipewire \
            --disable-jack --disable-esd --disable-sndio --disable-libsamplerate \
            --disable-loadso --disable-libudev --disable-dbus --disable-ibus --disable-fcitx \
            --disable-hidapi
        grep -qx '#define SDL_VIDEO_DRIVER_X11 1' include/SDL_config.h
        grep -qx '#define SDL_AUDIO_DRIVER_ALSA 1' include/SDL_config.h
        make -j"$jobs"
        make install
    )
fi

tar xf ffmpeg-7.1.1.tar.xz
cd ffmpeg-7.1.1
./configure \
    --disable-autodetect --enable-static --disable-shared \
    --disable-programs --disable-doc --disable-avfilter \
    --disable-network --disable-avdevice --disable-postproc --disable-x86asm \
    --disable-everything --enable-protocol=file \
    --enable-demuxer=mov,matroska,avi,mpegts \
    --enable-decoder=h264,hevc,av1,mpeg4,vp8,vp9,mpeg2video,aac,mp3,opus,vorbis,ac3,alac,flac,pcm_s16le \
    --enable-parser=h264,hevc,av1,vp8,vp9,mpeg4video,mpegvideo,aac,mpegaudio,opus,ac3
make -j"$jobs" libavformat/libavformat.a libavcodec/libavcodec.a \
    libswresample/libswresample.a libswscale/libswscale.a libavutil/libavutil.a


# Run bindgen as a build-time tool so libclang is never loaded by the player.
bindgen=${TO_VIEW_BINDGEN:-"$out/bindgen-tool/bin/bindgen"}
if [[ ! -x "$bindgen" ]]; then
    RUSTFLAGS='' CARGO_ENCODED_RUSTFLAGS='' cargo install --locked --version 0.72.1 \
        --target-dir "$out/bindgen-build" --root "$out/bindgen-tool" bindgen-cli
fi
"$bindgen" "$script_dir/native-bindings.h" --no-layout-tests --no-doc-comments --formatter=none \
    --allowlist-function '(av|swr|sws|SDL_).*' --allowlist-type 'AV.*|SDL_.*|Sw[rs].*' \
    --allowlist-var 'AV.*|SDL_.*|SWS_.*|AUDIO_.*|KMOD_.*|SDLK_.*' \
    --output "$out/bindings.rs" -- -I"$PWD" -I"$prefix/include/SDL2"
# Always emit Rust 2024 extern blocks, even with alternate bindgen binaries.
sed -Ei 's/(unsafe )?extern "C" \{/unsafe extern "C" {/g' "$out/bindings.rs"
