//! Streaming client for the NDJSON token endpoints (`…/send/stream`,
//! `…/regenerate/stream`).
//!
//! The browser can't use `EventSource` for these — those are GET-only and we
//! POST a body — so we drive the Fetch API's `ReadableStream` reader directly
//! and parse newline-delimited [`StreamMsg`] frames as they arrive, invoking a
//! callback per frame so the caller can grow the live bubble token by token.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use shared::dto::StreamMsg;

/// POST `body` to `url` and invoke `on_msg` for every NDJSON frame streamed back.
///
/// `cancel` is polled before each read; when it returns true the loop stops and
/// the underlying stream is cancelled. The server runs generation in a detached
/// task, so cancelling here only stops *rendering* — the full reply is still
/// persisted and appears on the next load.
pub async fn stream_post(
    url: &str,
    body: serde_json::Value,
    mut on_msg: impl FnMut(StreamMsg),
    cancel: impl Fn() -> bool,
) -> Result<(), String> {
    let win = web_sys::window().ok_or("no window")?;

    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    let headers = web_sys::Headers::new().map_err(|_| "could not build headers")?;
    let _ = headers.append("Content-Type", "application/json");
    init.set_headers(&headers);
    init.set_body(&JsValue::from_str(&body.to_string()));

    let request =
        web_sys::Request::new_with_str_and_init(url, &init).map_err(|_| "could not build request")?;
    let resp_val = JsFuture::from(win.fetch_with_request(&request))
        .await
        .map_err(|_| "network error".to_string())?;
    let resp: web_sys::Response = resp_val.dyn_into().map_err(|_| "bad response")?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let body_stream = resp.body().ok_or("response had no body")?;
    let reader: web_sys::ReadableStreamDefaultReader = body_stream
        .get_reader()
        .dyn_into()
        .map_err(|_| "could not read stream")?;

    let mut buf: Vec<u8> = Vec::new();
    loop {
        if cancel() {
            let _ = reader.cancel();
            break;
        }
        let read_val = JsFuture::from(reader.read())
            .await
            .map_err(|_| "stream read error".to_string())?;
        // The read result is `{ done: bool, value: Uint8Array }`.
        let done = js_sys::Reflect::get(&read_val, &JsValue::from_str("done"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&read_val, &JsValue::from_str("value"))
            .map_err(|_| "stream frame error")?;
        let chunk = js_sys::Uint8Array::new(&value).to_vec();
        buf.extend_from_slice(&chunk);
        // Drain every complete line (frames are `\n`-terminated). Splitting on
        // the `\n` byte is UTF-8 safe — it never falls inside a multibyte char.
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            dispatch_line(&line, &mut on_msg);
        }
    }
    // A trailing frame without a final newline.
    if !buf.is_empty() {
        dispatch_line(&buf, &mut on_msg);
    }
    Ok(())
}

fn dispatch_line(bytes: &[u8], on_msg: &mut impl FnMut(StreamMsg)) {
    let line = String::from_utf8_lossy(bytes);
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    if let Ok(msg) = serde_json::from_str::<StreamMsg>(line) {
        on_msg(msg);
    }
}
