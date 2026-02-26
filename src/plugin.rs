/// JSON-RPC 2.0 plugin for ClippyBot Mainframe.
///
/// Reads JSON-RPC requests from stdin (one per line), dispatches to the
/// image-compressor-rs library, and writes JSON-RPC responses to stdout.
/// All diagnostic output goes to stderr.
use image_compressor_rs::{
    CompressOptions, ResizeMode, ResizeOptions, compress_directory, compress_image_file,
    format_size,
};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::Path;

const NAME: &str = "image-compressor";
const VERSION: &str = "0.1.0";
const PROTOCOL_VERSION: &str = "2024-11-05";

fn main() {
    let stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();

    for line in stdin.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                log("error", &format!("stdin read error: {e}"));
                break;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                log("error", &format!("JSON parse error: {e}"));
                continue;
            }
        };

        let id = req.get("id").cloned();
        // Notifications (no id) — nothing to respond to
        if id.is_none() || id.as_ref().is_some_and(Value::is_null) {
            continue;
        }
        let id = id.unwrap();

        let method = req
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = req
            .get("params")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let response = match method {
            "initialize" => {
                log("info", "Plugin initialized");
                ok(&id, json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": NAME, "version": VERSION }
                }))
            }
            "ping" | "shutdown" => ok(&id, json!({})),
            "health/check" => ok(&id, json!({ "ok": true })),
            "tools/list" => ok(&id, json!({ "tools": tool_definitions() })),
            "tools/call" => handle_tool_call(&id, &params),
            _ => err(&id, -32601, &format!("Method not found: {method}")),
        };

        let _ = writeln!(stdout, "{}", response);
        let _ = stdout.flush();

        if method == "shutdown" {
            std::process::exit(0);
        }
    }
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

fn tool_definitions() -> Value {
    json!([
        {
            "name": "compress_image",
            "description": "Compress a single image file using best-in-class encoders (mozjpeg, oxipng, libwebp, ravif)",
            "inputSchema": {
                "type": "object",
                "required": ["input_path"],
                "properties": {
                    "input_path": {
                        "type": "string",
                        "description": "Path to the source image file"
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Path for the compressed output (format inferred from extension). Defaults to input path with format extension."
                    },
                    "quality": {
                        "type": "integer",
                        "description": "Compression quality 1-100 (default: format-specific — JPEG 85, WebP 85, AVIF 80)",
                        "minimum": 1,
                        "maximum": 100
                    },
                    "format": {
                        "type": "string",
                        "enum": ["jpeg", "png", "webp", "avif"],
                        "description": "Output format (overrides output_path extension)"
                    },
                    "max_width": {
                        "type": "integer",
                        "description": "Maximum width in pixels (maintains aspect ratio)",
                        "minimum": 1
                    },
                    "max_height": {
                        "type": "integer",
                        "description": "Maximum height in pixels (maintains aspect ratio)",
                        "minimum": 1
                    },
                    "lossless": {
                        "type": "boolean",
                        "description": "Use lossless compression (WebP and AVIF only, default: false)"
                    }
                }
            }
        },
        {
            "name": "compress_directory",
            "description": "Batch compress all images in a directory",
            "inputSchema": {
                "type": "object",
                "required": ["input_dir"],
                "properties": {
                    "input_dir": {
                        "type": "string",
                        "description": "Path to the source directory"
                    },
                    "output_dir": {
                        "type": "string",
                        "description": "Path for compressed output (defaults to input_dir + '_compressed')"
                    },
                    "quality": {
                        "type": "integer",
                        "description": "Compression quality 1-100 (default: 80)",
                        "minimum": 1,
                        "maximum": 100
                    },
                    "format": {
                        "type": "string",
                        "enum": ["jpeg", "png", "webp", "avif"],
                        "description": "Output format for all images (default: webp)"
                    }
                }
            }
        }
    ])
}

// ---------------------------------------------------------------------------
// Tool dispatch
// ---------------------------------------------------------------------------

fn handle_tool_call(id: &Value, params: &Value) -> Value {
    let tool_name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match tool_name {
        "compress_image" => call_compress_image(id, &args),
        "compress_directory" => call_compress_directory(id, &args),
        _ => err(id, -32601, &format!("Unknown tool: {tool_name}")),
    }
}

fn call_compress_image(id: &Value, args: &Value) -> Value {
    let Some(input_path) = args.get("input_path").and_then(Value::as_str) else {
        return err(id, -32602, "Missing required parameter: input_path");
    };

    let format_ext = args
        .get("format")
        .and_then(Value::as_str)
        .map(|f| match f {
            "jpeg" => "jpg",
            other => other,
        });

    let output_path = match args.get("output_path").and_then(Value::as_str) {
        Some(p) => p.to_string(),
        None => {
            let ext = format_ext.unwrap_or("webp");
            let p = Path::new(input_path);
            p.with_extension(ext).to_string_lossy().into_owned()
        }
    };

    // If format is specified, override the output extension
    let final_output = if let Some(ext) = format_ext {
        let p = Path::new(&output_path);
        p.with_extension(ext).to_string_lossy().into_owned()
    } else {
        output_path
    };

    let max_width = args.get("max_width").and_then(Value::as_u64).map(|v| v as u32);
    let max_height = args.get("max_height").and_then(Value::as_u64).map(|v| v as u32);

    let resize = match (max_width, max_height) {
        (Some(w), Some(h)) => ResizeOptions::new(w, h, ResizeMode::Fit).ok(),
        (Some(w), None) => ResizeOptions::new(w, u32::MAX, ResizeMode::Fit).ok(),
        (None, Some(h)) => ResizeOptions::new(u32::MAX, h, ResizeMode::Fit).ok(),
        _ => None,
    };

    let options = CompressOptions {
        overwrite: true,
        quality: args.get("quality").and_then(Value::as_u64).map(|v| v as u8),
        lossless: args.get("lossless").and_then(Value::as_bool).unwrap_or(false),
        resize,
        ..CompressOptions::default()
    };

    log("info", &format!("compress_image: {input_path} -> {final_output}"));

    match compress_image_file(Path::new(input_path), Path::new(&final_output), &options) {
        Ok(stats) => {
            let text = format!(
                "Compressed {} -> {} ({} -> {}, saved {:.1}%)",
                input_path,
                final_output,
                format_size(stats.original_bytes),
                format_size(stats.compressed_bytes),
                stats.savings_percent,
            );
            ok(id, json!({
                "content": [{ "type": "text", "text": text }]
            }))
        }
        Err(e) => err(id, -32000, &format!("Compression failed: {e:#}")),
    }
}

fn call_compress_directory(id: &Value, args: &Value) -> Value {
    let Some(input_dir) = args.get("input_dir").and_then(Value::as_str) else {
        return err(id, -32602, "Missing required parameter: input_dir");
    };

    let format_ext = args
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("webp");
    let format_ext = match format_ext {
        "jpeg" => "jpg",
        other => other,
    };

    let output_dir = match args.get("output_dir").and_then(Value::as_str) {
        Some(p) => p.to_string(),
        None => format!("{input_dir}_compressed"),
    };

    let quality = args.get("quality").and_then(Value::as_u64).map(|v| v as u8);

    let options = CompressOptions {
        overwrite: true,
        quality,
        ..CompressOptions::default()
    };

    log("info", &format!("compress_directory: {input_dir} -> {output_dir} (format: {format_ext})"));

    match compress_directory(
        Path::new(input_dir),
        Path::new(&output_dir),
        format_ext,
        &options,
        true,
    ) {
        Ok(report) => {
            let text = format!(
                "Batch compression complete: {} compressed, {} skipped, {} failed ({} -> {})",
                report.compressed,
                report.skipped,
                report.failed,
                format_size(report.total_original_bytes),
                format_size(report.total_compressed_bytes),
            );
            ok(id, json!({
                "content": [{ "type": "text", "text": text }]
            }))
        }
        Err(e) => err(id, -32000, &format!("Batch compression failed: {e:#}")),
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

fn ok(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn err(id: &Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn log(level: &str, msg: &str) {
    let entry = json!({
        "ts": chrono_now(),
        "level": level,
        "plugin": NAME,
        "msg": msg
    });
    eprintln!("{entry}");
}

fn chrono_now() -> String {
    // Simple ISO-ish timestamp without pulling in chrono
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    format!("{secs}")
}
