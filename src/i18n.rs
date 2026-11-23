use gettext::Catalog;
use std::collections::HashMap;
use std::{collections::HashSet, env, ffi::CString, fs, io::Cursor, path::PathBuf, sync::OnceLock};

const RU: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/i18n/ru.mo"));
const EN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/i18n/en.mo"));
const KK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/i18n/kk.mo"));

struct Translations {
  current: Loaded,
  russian: Loaded,
}

#[derive(Clone)]
struct Loaded {
  catalog: Catalog,
  keys: HashSet<String>,
  plurals: HashMap<String, Vec<String>>,
  rule: Option<gettext_catalog::plural::PluralRule>,
}

static TRANSLATIONS: OnceLock<Translations> = OnceLock::new();

fn parse(data: &[u8]) -> Option<Loaded> {
  if data.len() < 28 || data.len() > 16 * 1024 * 1024 {
    return None;
  }
  let big = match &data[..4] {
    [0xde, 0x12, 0x04, 0x95] => false,
    [0x95, 0x04, 0x12, 0xde] => true,
    _ => return None,
  };
  let read = |offset: usize| -> Option<usize> {
    let bytes: [u8; 4] = data.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(if big {
      u32::from_be_bytes(bytes)
    } else {
      u32::from_le_bytes(bytes)
    } as usize)
  };
  if read(4)? >> 16 > 1 {
    return None;
  }
  let count = read(8)?;
  if count > data.len() / 8 {
    return None;
  }
  for table in [read(12)?, read(16)?] {
    if table.checked_add(count.checked_mul(8)?)? > data.len() {
      return None;
    }
    for n in 0..count {
      let length = read(table + n * 8)?;
      let offset = read(table + n * 8 + 4)?;
      if *data.get(offset.checked_add(length)?)? != 0 {
        return None;
      }
    }
  }
  let original_table = read(12)?;
  let mut keys = HashSet::new();
  for n in 0..count {
    let length = read(original_table + n * 8)?;
    let offset = read(original_table + n * 8 + 4)?;
    let bytes = data.get(offset..offset + length)?;
    let key = bytes.split(|&byte| byte == 0).next()?;
    if let Ok(key) = std::str::from_utf8(key) {
      keys.insert(key.to_owned());
    }
  }
  let catalog = std::panic::catch_unwind(|| {
    gettext::ParseOptions::new()
      .force_plural(|n| usize::from(n != 1))
      .parse(Cursor::new(data))
      .ok()
  })
  .ok()
  .flatten()?;
  let header = catalog.gettext("");
  let header_value = |key: &str| {
    header.lines().find_map(|line| {
      let (name, value) = line.split_once(':')?;
      name.eq_ignore_ascii_case(key).then_some(value.trim())
    })
  };
  let encoding_name = header_value("Content-Type")
    .and_then(|value| {
      value
        .split(';')
        .find_map(|part| part.trim().strip_prefix("charset="))
    })
    .unwrap_or("UTF-8");
  let codec = encoding::label::encoding_from_whatwg_label(encoding_name)?;
  let rule = header_value("Plural-Forms").and_then(|value| {
    if value.len() > 512 {
      return None;
    }
    let mut count = None;
    let mut expression = None;
    for part in value.split(';') {
      if let Some((key, value)) = part.split_once('=') {
        match key.trim() {
          "nplurals" => {
            count = value
              .trim()
              .parse::<usize>()
              .ok()
              .filter(|n| (1..=16).contains(n))
          }
          "plural" => expression = Some(value.trim()),
          _ => {}
        }
      }
    }
    let expression = expression?;
    if expression.len() > 256 || expression.chars().filter(|ch| *ch == '(').count() > 32 {
      return None;
    }
    gettext_catalog::plural::PluralRule::new(count?, expression).ok()
  });
  let mut plurals = HashMap::new();
  let translated_table = read(16)?;
  for n in 0..count {
    let length = read(original_table + n * 8)?;
    let offset = read(original_table + n * 8 + 4)?;
    let original = data.get(offset..offset + length)?;
    if !original.contains(&0) {
      continue;
    }
    let key = std::str::from_utf8(original.split(|&byte| byte == 0).next()?).ok()?;
    let length = read(translated_table + n * 8)?;
    let offset = read(translated_table + n * 8 + 4)?;
    let bytes = data.get(offset..offset + length)?;
    let forms = bytes
      .split(|&byte| byte == 0)
      .map(|part| codec.decode(part, encoding::DecoderTrap::Strict).ok())
      .collect::<Option<Vec<_>>>()?;
    plurals.insert(key.to_owned(), forms);
  }
  Some(Loaded {
    catalog,
    keys,
    plurals,
    rule,
  })
}

fn locale_name(value: &str) -> Option<(String, Vec<String>, bool)> {
  let value = value.trim();
  if value.is_empty()
    || value.len() > 128
    || value
      .bytes()
      .any(|b| !b.is_ascii_alphanumeric() && !b"_.@-".contains(&b))
  {
    return None;
  }
  if matches!(
    value.to_ascii_lowercase().as_str(),
    "c" | "posix" | "c.utf8" | "c.utf-8"
  ) {
    return None;
  }
  let (main, modifier) = match value.split_once('@') {
    Some((a, b)) if !b.is_empty() && !b.contains('@') => (a, Some(b)),
    Some(_) => return None,
    None => (value, None),
  };
  let main = match main.split_once('.') {
    Some((left, encoding)) if !encoding.is_empty() && !encoding.contains('.') => left,
    Some(_) => return None,
    None => main,
  };
  let (lang, region) = match main.split_once('_') {
    Some((a, b)) if (2..=3).contains(&b.len()) && b.bytes().all(|c| c.is_ascii_alphanumeric()) => {
      (a, Some(b.to_ascii_uppercase()))
    }
    Some(_) => return None,
    None => (main, None),
  };
  if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_alphabetic()) {
    return None;
  }
  let lang = lang.to_ascii_lowercase();
  let mut names = Vec::new();
  if let Some(modifier) = modifier {
    if let Some(region) = &region {
      names.push(format!("{}_{region}@{modifier}", lang));
    }
    names.push(format!("{lang}@{modifier}"));
  }
  if let Some(region) = &region {
    names.push(format!("{}_{region}", lang));
  }
  names.push(lang.clone());
  Some((lang, names, value == main && region.is_none()))
}

fn installed(name: &str) -> bool {
  let Ok(name) = CString::new(name) else {
    return false;
  };
  unsafe {
    let locale = libc::newlocale(libc::LC_MESSAGES_MASK, name.as_ptr(), std::ptr::null_mut());
    if locale.is_null() {
      return false;
    }
    libc::freelocale(locale);
  }
  true
}

fn candidate(value: &str, list: bool) -> Option<Loaded> {
  candidate_with(
    value,
    list,
    &["/usr/local/share/locale", "/usr/share/locale"],
    installed,
  )
}

fn candidate_with(
  value: &str,
  list: bool,
  roots: &[&str],
  is_installed: impl Fn(&str) -> bool,
) -> Option<Loaded> {
  let (language, names, bare) = locale_name(value)?;
  if !(list && bare) && !is_installed(value.trim()) {
    return None;
  }
  match language.as_str() {
    "ru" => return parse(RU),
    "en" => return parse(EN),
    "kk" => return parse(KK),
    _ => {}
  }
  for name in names {
    for root in roots {
      let path = PathBuf::from(root)
        .join(&name)
        .join("LC_MESSAGES/toview.mo");
      if let Ok(data) = fs::read(path)
        && let Some(catalog) = parse(&data)
      {
        return Some(catalog);
      }
    }
  }
  None
}

pub fn init() {
  let russian = parse(RU).expect("invalid embedded Russian translation");
  let current = ["LC_ALL", "LC_MESSAGES", "LANG"]
    .iter()
    .find_map(|name| {
      env::var(name)
        .ok()
        .and_then(|value| candidate(&value, false))
    })
    .or_else(|| {
      env::var("LANGUAGES")
        .ok()
        .and_then(|values| values.split(':').find_map(|value| candidate(value, true)))
    })
    .unwrap_or_else(|| russian.clone());
  let _ = TRANSLATIONS.set(Translations { current, russian });
}

pub fn tr(key: &str) -> String {
  let Some(t) = TRANSLATIONS.get() else {
    return key.to_owned();
  };
  let result = t.current.catalog.gettext(key);
  if !t.current.keys.contains(key) || result.is_empty() {
    t.russian.catalog.gettext(key).to_owned()
  } else {
    result.to_owned()
  }
}

#[allow(dead_code)]
pub fn contextual(context: &str, key: &str) -> String {
  let Some(t) = TRANSLATIONS.get() else {
    return key.to_owned();
  };
  let combined = format!("{context}\u{4}{key}");
  if t.current.keys.contains(&combined) {
    let translated = t.current.catalog.pgettext(context, key);
    if !translated.is_empty() {
      return translated.to_owned();
    }
  }
  t.russian.catalog.pgettext(context, key).to_owned()
}

#[allow(dead_code)]
pub fn plural(key: &str, plural_key: &str, n: u64) -> String {
  let Some(t) = TRANSLATIONS.get() else {
    return if n == 1 { key } else { plural_key }.to_owned();
  };
  let lookup = |catalog: &Loaded| {
    let rule = catalog.rule.as_ref()?;
    let forms = catalog.plurals.get(key)?;
    forms
      .get(rule.index(n))
      .filter(|value| !value.is_empty())
      .cloned()
  };
  lookup(&t.current)
    .or_else(|| lookup(&t.russian))
    .unwrap_or_else(|| {
      if n == 1 {
        key.to_owned()
      } else {
        plural_key.to_owned()
      }
    })
}

pub fn format(key: &str, args: &[(&str, &str)]) -> String {
  let fallback = TRANSLATIONS
    .get()
    .map(|t| t.russian.catalog.gettext(key))
    .unwrap_or(key);
  let translated = tr(key);
  let placeholders = |text: &str| {
    let mut items = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
      rest = &rest[start + 1..];
      let Some(end) = rest.find('}') else {
        return None;
      };
      items.push(rest[..end].to_owned());
      rest = &rest[end + 1..];
    }
    items.sort();
    Some(items)
  };
  let chosen = if placeholders(&translated) == placeholders(key) {
    translated.as_str()
  } else {
    fallback
  };
  let mut out = chosen.to_owned();
  for (name, value) in args {
    out = out.replace(&format!("{{{name}}}"), value);
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn locale_parsing_and_catalog_integrity() {
    assert_eq!(
      locale_name("sr_RS.UTF-8@latin").unwrap().1,
      ["sr_RS@latin", "sr@latin", "sr_RS", "sr"]
    );
    for invalid in [
      "C",
      "C.UTF-8",
      "",
      "../ru",
      "ru/../../x",
      "ru.",
      "en_US.foo.bar",
    ] {
      assert!(locale_name(invalid).is_none(), "{invalid}");
    }
    assert_eq!(
      locale_name("SR_rs.UTF-8@latin").unwrap().1,
      ["sr_RS@latin", "sr@latin", "sr_RS", "sr"]
    );
    for bytes in [RU, EN, KK] {
      let catalog = parse(bytes).unwrap();
      for key in ["Show help", "Frame: {frame}", "unknown option {option}"] {
        assert!(catalog.keys.contains(key));
        assert!(!catalog.catalog.gettext(key).is_empty());
      }
    }
    for (bytes, count, expected) in [
      (RU, 1, "{count} кадр"),
      (RU, 2, "{count} кадра"),
      (RU, 5, "{count} кадров"),
      (EN, 2, "{count} frames"),
      (KK, 5, "{count} кадр"),
    ] {
      let catalog = parse(bytes).unwrap();
      let index = catalog.rule.as_ref().unwrap().index(count);
      assert_eq!(catalog.plurals["{count} frame"][index], expected);
    }
    assert!(parse(b"bad").is_none());
    let mut bad = RU.to_vec();
    bad[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse(&bad).is_none());
  }

  #[test]
  fn locale_selection_tries_installed_and_external_catalogs() {
    assert!(candidate_with("ru_RU.UTF-8", false, &[], |_| false).is_none());
    assert_eq!(
      candidate_with("kk", true, &[], |_| false)
        .unwrap()
        .catalog
        .gettext("Show help"),
      "Анықтаманы көрсету"
    );
    let dir = std::env::temp_dir().join(format!("toview-i18n-test-{}", std::process::id()));
    let path = dir.join("fr/LC_MESSAGES");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("toview.mo"), KK).unwrap();
    let root = dir.to_str().unwrap();
    assert_eq!(
      candidate_with("fr_FR.UTF-8", false, &[root], |_| true)
        .unwrap()
        .catalog
        .gettext("Show help"),
      "Анықтаманы көрсету"
    );
    assert!(candidate_with("fr_FR.UTF-8", false, &[root], |_| false).is_none());
    std::fs::remove_dir_all(dir).unwrap();
  }
}
