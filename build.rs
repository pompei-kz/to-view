use std::{env, fs, path::PathBuf, process::Command};

fn main() {
  for language in ["ru", "en", "kk"] {
    let source = format!("locales/{language}/LC_MESSAGES/toview.po");
    println!("cargo:rerun-if-changed={source}");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("i18n");
    fs::create_dir_all(&output).expect("create built-in translations directory");
    let target = output.join(format!("{language}.mo"));
    if target.exists() {
      fs::remove_file(&target).expect("remove outdated translation");
    }
    let status = Command::new("msgfmt")
      .args(["--check", "--check-format", "-o"])
      .arg(&target)
      .arg(&source)
      .status()
      .expect("msgfmt is required to build translations; install gettext");
    assert!(status.success(), "failed to compile {source}");
  }
  validate_catalogs();

  println!("cargo:rerun-if-changed=scripts/build-native.sh");
  println!("cargo:rerun-if-changed=scripts/link-native.sh");
  println!("cargo:rerun-if-changed=scripts/native-bindings.h");
  println!("cargo:rerun-if-env-changed=TOVIEW_X11_SYSROOT");
  println!("cargo:rerun-if-env-changed=TOVIEW_BUILD_JOBS");
  println!("cargo:rerun-if-env-changed=TOVIEW_BINDGEN");

  assert_eq!(
    env::var("TARGET").unwrap(),
    env::var("HOST").unwrap(),
    "Only native Linux GNU builds are supported"
  );
  assert!(
    env::var("TARGET").unwrap().ends_with("-linux-gnu"),
    "toview requires Linux with a GNU build toolchain"
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
  println!("cargo:rustc-link-arg=-static-pie");
  println!(
    "cargo:rustc-link-arg=-Wl,-Bstatic,--wrap=snd_pcm_open,--wrap=dlopen,--wrap=__libc_dlopen_mode,--start-group"
  );
  for name in ["avformat", "avcodec", "swresample", "swscale", "avutil"] {
    println!(
      "cargo:rustc-link-arg={}",
      out
        .join(format!("ffmpeg-7.1.1/lib{name}/lib{name}.a"))
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

fn validate_catalogs() {
  use std::{collections::BTreeSet, io::Cursor};
  let base = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("i18n");
  let read_catalog = |language: &str| {
    let bytes = fs::read(base.join(format!("{language}.mo"))).expect("read compiled translation");
    let read =
      |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    let count = read(8);
    let originals = read(12);
    let mut keys = BTreeSet::new();
    for index in 0..count {
      let length = read(originals + index * 8);
      let offset = read(originals + index * 8 + 4);
      let key = std::str::from_utf8(&bytes[offset..offset + length])
        .expect("UTF-8 msgid")
        .split('\0')
        .next()
        .unwrap();
      if !key.is_empty() {
        keys.insert(key.to_owned());
      }
    }
    let catalog = gettext::Catalog::parse(Cursor::new(bytes)).expect("parse compiled translation");
    (keys, catalog)
  };
  let (keys, _) = read_catalog("en");
  for language in ["ru", "en", "kk"] {
    let (translated_keys, catalog) = read_catalog(language);
    assert_eq!(
      keys, translated_keys,
      "translation keys differ in {language}"
    );
    for key in &keys {
      let value = catalog.gettext(key);
      assert!(!value.is_empty(), "empty translation in {language}: {key}");
      let placeholders = |text: &str| {
        let mut found = Vec::new();
        let mut remaining = text;
        while let Some(start) = remaining.find('{') {
          remaining = &remaining[start + 1..];
          let end = remaining
            .find('}')
            .expect("unclosed translation placeholder");
          found.push(remaining[..end].to_owned());
          remaining = &remaining[end + 1..];
        }
        found.sort();
        found
      };
      assert_eq!(
        placeholders(key),
        placeholders(value),
        "translation placeholders differ in {language}: {key}"
      );
    }
  }
}
