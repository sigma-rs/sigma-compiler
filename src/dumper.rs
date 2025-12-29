#![cfg(feature = "dump")]

//! If the `dump` feature is enabled, expose an API to optionally
//! allow dumping to a `String` instead of to stdout.  This is
//! useful if you're running in wasm, which doesn't support stdout.

use std::sync::Mutex;

/// If `None` (the default), dump to stdout.  If `Some(s)`, dump by
/// appending to `s`
static DUMP_BUFFER: Mutex<Option<String>> = Mutex::new(None);

/// Enable dumping to a `String` (default is to dump to stdout)
pub fn dump_to_string() {
    *DUMP_BUFFER.lock().unwrap() = Some(String::new());
}

/// Retrieve (and clear) the `String` dump buffer
pub fn dump_buffer() -> String {
    let mut b = DUMP_BUFFER.lock().unwrap();
    match *b {
        None => String::new(),
        Some(ref mut buf) => std::mem::take(buf),
    }
}

/// Dump a `&str` to either stdout or the `String` buffer
pub fn dump(s: &str) {
    let mut b = DUMP_BUFFER.lock().unwrap();
    match *b {
        None => {
            print!("{}", s);
        }
        Some(ref mut buf) => {
            buf.push_str(s);
        }
    }
}
