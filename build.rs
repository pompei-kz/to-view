use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=scripts/build-native.sh");
    println!("cargo:rerun-if-changed=scripts/ffplay-exit-status.patch");
    println!("cargo:rerun-if-env-changed=TO_VIEW_X11_SYSROOT");
    println!("cargo:rerun-if-env-changed=TO_VIEW_BUILD_JOBS");

    assert_eq!(
        env::var("TARGET").unwrap(),
        env::var("HOST").unwrap(),
        "Only native Linux GNU builds are supported"
    );
    assert!(
        env::var("TARGET").unwrap().ends_with("-linux-gnu"),
        "to-view requires Linux with a GNU build toolchain"
    );
    assert!(
        env::var("CARGO_CFG_TARGET_FEATURE")
            .unwrap_or_default()
            .split(',')
            .any(|feature| feature == "crt-static"),
        "to-view requires +crt-static"
    );

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("native");
    let status = Command::new("bash")
        .arg("scripts/build-native.sh")
        .arg(&out)
        .status()
        .expect("could not start native build");
    assert!(status.success(), "static native build failed");

    let xlib =
        PathBuf::from(fs::read_to_string(out.join("xlib-path")).expect("missing X11 library path"));

    println!("cargo:rustc-link-arg=-fuse-ld=bfd");
    println!(
        "cargo:rustc-link-arg=-Wl,-Bstatic,--wrap=SDL_CreateWindow,--wrap=snd_pcm_open,--wrap=dlopen,--wrap=__libc_dlopen_mode,--start-group"
    );
    println!("cargo:rustc-link-arg={}", out.join("libffplay.a").display());
    for name in [
        "avformat",
        "avcodec",
        "avfilter",
        "swresample",
        "swscale",
        "avutil",
    ] {
        println!(
            "cargo:rustc-link-arg={}",
            out.join(format!("ffmpeg-7.1.1/lib{name}/lib{name}.a"))
                .display()
        );
    }
    for name in ["SDL2", "asound"] {
        println!(
            "cargo:rustc-link-arg={}",
            out.join(format!("install/lib/lib{name}.a")).display()
        );
    }
    for name in ["X11", "Xext", "xcb", "Xau", "Xdmcp"] {
        println!(
            "cargo:rustc-link-arg={}",
            xlib.join(format!("lib{name}.a")).display()
        );
    }
    for library in ["-lm", "-lc", "-lgcc_eh", "-lgcc"] {
        println!("cargo:rustc-link-arg={library}");
    }
    println!("cargo:rustc-link-arg=-Wl,--end-group");
}
