//! Read a browser `File` into a base64 `data:` URL so images can be uploaded
//! directly from the user's device (no external hosting / hotlink issues).
//!
//! The result is a string like `data:image/png;base64,iVBOR...` that can be
//! dropped straight into an `<img src>` and persisted in the database.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{File, FileReader};

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
