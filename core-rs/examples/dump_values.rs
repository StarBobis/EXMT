//! Flatten every decoded object into leaf lines, for cross-checking values.
//!
//! Usage: `cargo run --quiet --example dump_values -- <bundle>`
//!
//! Line format: `<object>/<path>|<kind>|<value>`
//!
//! * `i32`/`i64`/`u32`/`u64`/`bool` print decimal.
//! * `f32`/`f64` print their raw bit pattern in hex, so the comparison never
//!   depends on float formatting.
//! * `str` prints the UTF-8 bytes in hex (empty string is `-`).
//! * `bytes` and `array` print their length only; elements follow as their own
//!   lines with `[i]` in the path.

use std::process::ExitCode;

use exmt_core::typetree::Value;
use exmt_core::{container, serialized, typetree};

fn hex(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn walk(out: &mut String, path: &str, value: &Value) {
    match value {
        Value::Bool(v) => out.push_str(&format!("{path}|bool|{v}\n")),
        // Integer width is not compared: a dynamic-language reader reports every
        // integer as one type, so keeping I32/U32 distinct here would flag every
        // field as a difference without saying anything about the data.
        Value::I8(v) => out.push_str(&format!("{path}|int|{v}\n")),
        Value::U8(v) => out.push_str(&format!("{path}|int|{v}\n")),
        Value::I16(v) => out.push_str(&format!("{path}|int|{v}\n")),
        Value::U16(v) => out.push_str(&format!("{path}|int|{v}\n")),
        Value::I32(v) => out.push_str(&format!("{path}|int|{v}\n")),
        Value::U32(v) => out.push_str(&format!("{path}|int|{v}\n")),
        Value::I64(v) => out.push_str(&format!("{path}|int|{v}\n")),
        Value::U64(v) => out.push_str(&format!("{path}|int|{v}\n")),
        Value::F32(v) => out.push_str(&format!("{path}|f32|{:08x}\n", v.to_bits())),
        Value::F64(v) => out.push_str(&format!("{path}|f64|{:016x}\n", v.to_bits())),
        Value::Str(s) => {
            let bytes = s.as_bytes();
            let encoded = if bytes.is_empty() { "-".to_string() } else { hex(bytes) };
            out.push_str(&format!("{path}|str|{}\n", encoded));
        }
        Value::Bytes(b) => out.push_str(&format!("{path}|bytes|{}\n", b.len())),
        Value::Array(items) => {
            out.push_str(&format!("{path}|array|{}\n", items.len()));
            for (i, item) in items.iter().enumerate() {
                walk(out, &format!("{path}[{i}]"), item);
            }
        }
        Value::Struct(fields) => {
            out.push_str(&format!("{path}|struct|{}\n", fields.len()));
            for (name, child) in fields {
                walk(out, &format!("{path}.{name}"), child);
            }
        }
        Value::Pair(a, b) => {
            out.push_str(&format!("{path}|pair|-\n"));
            walk(out, &format!("{path}.0"), a);
            walk(out, &format!("{path}.1"), b);
        }
    }
}

fn run(path: &str) -> Result<String, String> {
    let raw = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let plain = container::decrypt(&raw).map_err(|e| format!("decrypt: {e}"))?;
    let info = container::parse(&plain).map_err(|e| format!("container: {e}"))?;
    let nodes = container::node_payloads(&plain, &info).map_err(|e| format!("nodes: {e}"))?;
    let sf = serialized::SerializedFile::parse(&nodes[0].data).map_err(|e| format!("serialized: {e}"))?;

    let mut out = String::new();
    for (path_id, obj) in &sf.objects {
        let value = typetree::read_value(&sf, obj).map_err(|e| format!("object {path_id}: {e}"))?;
        walk(&mut out, &format!("obj{path_id}"), &value);
    }
    Ok(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: {} <bundle>", args[0]);
        return ExitCode::FAILURE;
    }
    match run(&args[1]) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
