#![allow(
  non_camel_case_types,
  non_snake_case,
  non_upper_case_globals,
  dead_code,
  unnecessary_transmutes
)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/native/bindings.rs"));
