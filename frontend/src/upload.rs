//! Read a browser `File` into a base64 `data:` URL so images can be uploaded
//! directly from the user's device (no external hosting / hotlink issues).
//!
//! The result is a string like `data:image/png;base64,iVBOR...` that can be
//! dropped straight into an `<img src>` and persisted in the database.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    CanvasRenderingContext2d, File, FileReader, HtmlCanvasElement, HtmlImageElement,
};

/// Read `file` as a data URL, invoking `on_done` with the result (or an error
/// message). Runs asynchronously via the browser's `FileReader`. Both the
/// success and error paths free the one-shot closures (and the `FileReader`),
/// so nothing leaks per upload.
pub fn read_as_data_url<F>(file: File, on_done: F)
where
    F: Fn(Result<String, String>) + 'static,
{
    let reader = match FileReader::new() {
        Ok(r) => r,
        Err(_) => {
            on_done(Err("could not create file reader".into()));
            return;
        }
    };

    let on_done = Rc::new(on_done);
    // Holds both closures alive until exactly one of load/error fires, then is
    // cleared so they (and the captured FileReader) drop.
    type Holder = Rc<RefCell<Option<(Closure<dyn FnMut()>, Closure<dyn FnMut()>)>>>;
    let holder: Holder = Rc::new(RefCell::new(None));

    let reader_load = reader.clone();
    let on_done_load = on_done.clone();
    let holder_load = holder.clone();
    let onload = Closure::<dyn FnMut()>::new(move || {
        match reader_load.result() {
            Ok(val) => match val.as_string() {
                Some(s) => on_done_load(Ok(s)),
                None => on_done_load(Err("file reader returned non-string".into())),
            },
            Err(_) => on_done_load(Err("file reader error".into())),
        }
        holder_load.borrow_mut().take(); // free both closures + the FileReader
    });

    let on_done_err = on_done.clone();
    let holder_err = holder.clone();
    let onerror = Closure::<dyn FnMut()>::new(move || {
        on_done_err(Err("could not read the selected file".into()));
        holder_err.borrow_mut().take();
    });

    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    reader.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    *holder.borrow_mut() = Some((onload, onerror));

    if reader.read_as_data_url(&file).is_err() {
        on_done(Err("could not start file read".into()));
        holder.borrow_mut().take();
    }
}

/// Read `file` as raw bytes (used for PNG character-card import). Same one-shot
/// closure-freeing pattern as [`read_as_data_url`].
pub fn read_as_bytes<F>(file: File, on_done: F)
where
    F: Fn(Result<Vec<u8>, String>) + 'static,
{
    let reader = match FileReader::new() {
        Ok(r) => r,
        Err(_) => {
            on_done(Err("could not create file reader".into()));
            return;
        }
    };

    let on_done = Rc::new(on_done);
    type Holder = Rc<RefCell<Option<(Closure<dyn FnMut()>, Closure<dyn FnMut()>)>>>;
    let holder: Holder = Rc::new(RefCell::new(None));

    let reader_load = reader.clone();
    let on_done_load = on_done.clone();
    let holder_load = holder.clone();
    let onload = Closure::<dyn FnMut()>::new(move || {
        match reader_load.result() {
            Ok(val) => {
                let buf = js_sys::Uint8Array::new(&val);
                on_done_load(Ok(buf.to_vec()));
            }
            Err(_) => on_done_load(Err("file reader error".into())),
        }
        holder_load.borrow_mut().take();
    });

    let on_done_err = on_done.clone();
    let holder_err = holder.clone();
    let onerror = Closure::<dyn FnMut()>::new(move || {
        on_done_err(Err("could not read the selected file".into()));
        holder_err.borrow_mut().take();
    });

    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    reader.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    *holder.borrow_mut() = Some((onload, onerror));

    if reader.read_as_array_buffer(&file).is_err() {
        on_done(Err("could not start file read".into()));
        holder.borrow_mut().take();
    }
}

/// Read an image `File`, downscale it client-side to at most `max_dim` px on the
/// longest edge, and hand back a compressed JPEG `data:` URL. This removes any
/// practical upload-size limit — a multi-megabyte photo becomes a small avatar.
///
/// Robust by design: if any step of the resize path fails (no canvas, decode
/// error, unsupported export), it falls back to the *original* data URL, so the
/// upload always succeeds with no hard cap.
pub fn read_image_scaled<F>(file: File, max_dim: u32, on_done: F)
where
    F: Fn(Result<String, String>) + 'static,
{
    let on_done = Rc::new(on_done);
    let done = on_done.clone();
    // Step 1: read the raw file into a data URL we can feed to an <img>.
    read_as_data_url(file, move |res| {
        let data_url = match res {
            Ok(d) => d,
            Err(e) => {
                done(Err(e));
                return;
            }
        };
        // Step 2: decode it in an off-DOM <img>, then draw it scaled to a canvas.
        let img = match HtmlImageElement::new() {
            Ok(i) => i,
            Err(_) => {
                done(Ok(data_url)); // can't resize — use the original
                return;
            }
        };

        // Keep both load/error closures alive until exactly one fires.
        type Holder = Rc<RefCell<Option<(Closure<dyn FnMut()>, Closure<dyn FnMut()>)>>>;
        let holder: Holder = Rc::new(RefCell::new(None));

        let img_load = img.clone();
        let done_load = done.clone();
        let original_load = data_url.clone();
        let holder_load = holder.clone();
        let onload = Closure::<dyn FnMut()>::new(move || {
            let out =
                downscale_to_jpeg(&img_load, max_dim).unwrap_or_else(|| original_load.clone());
            done_load(Ok(out));
            holder_load.borrow_mut().take();
        });

        let done_err = done.clone();
        let original_err = data_url.clone();
        let holder_err = holder.clone();
        let onerror = Closure::<dyn FnMut()>::new(move || {
            // Decode failed — fall back to the original data URL.
            done_err(Ok(original_err.clone()));
            holder_err.borrow_mut().take();
        });

        img.set_onload(Some(onload.as_ref().unchecked_ref()));
        img.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        *holder.borrow_mut() = Some((onload, onerror));
        img.set_src(&data_url);
    });
}

/// Draw `img` onto a canvas scaled to fit `max_dim` on its longest edge and
/// export as JPEG. Returns `None` on any failure so the caller can fall back.
fn downscale_to_jpeg(img: &HtmlImageElement, max_dim: u32) -> Option<String> {
    let w = img.natural_width();
    let h = img.natural_height();
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (max_dim as f64 / w.max(h) as f64).min(1.0);
    let nw = ((w as f64) * scale).round().max(1.0);
    let nh = ((h as f64) * scale).round().max(1.0);

    let document = web_sys::window()?.document()?;
    let canvas: HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
    canvas.set_width(nw as u32);
    canvas.set_height(nh as u32);
    let ctx: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.dyn_into().ok()?;
    ctx.draw_image_with_html_image_element_and_dw_and_dh(img, 0.0, 0.0, nw, nh)
        .ok()?;
    // JPEG keeps avatars tiny; 0.82 is visually clean at avatar sizes.
    canvas
        .to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(0.82))
        .ok()
}
