//! Android: the Activity's pen hook.
//!
//! winit's Android backend forwards a pen's pressure as touch force but drops its tilt,
//! orientation and tool type (tip or eraser). The app's Activity (`PhotoCraftActivity.java`)
//! reads them from every `MotionEvent` before the event goes on to GameActivity and winit, and
//! calls the `native` method below for each pointer and historical sample. Only primitives
//! cross JNI: no `JNIEnv` call is made, so nothing here dereferences a pointer.
//!
//! The one `unsafe` in this module is the `#[unsafe(no_mangle)]` attribute itself, which JNI's
//! name-based lookup (`Java_<package>_<class>_<method>`) needs.

#![allow(unsafe_code)]

use crate::motion::{RawEvent, handle};
use crate::{Error, Sample, Update, deliver};
use std::ffi::c_void;
use std::sync::{Mutex, PoisonError};

type Sink = Box<dyn Fn(Option<Sample>) + Send + Sync>;

/// Where pen samples go; set by [`install`].
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Route the Activity's pen samples to `callback` (replacing an earlier one).
pub fn install(callback: impl Fn(Option<Sample>) + Send + Sync + 'static) -> Result<(), Error> {
    *SINK.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(callback));
    Ok(())
}

/// One pointer of a `MotionEvent`, from `PhotoCraftActivity.nativePen` (Java). Called on the
/// Activity's UI thread. JNI types: `jint` is `i32`, `jfloat` is `f32`.
///
/// Never unwinds: the mapping doesn't panic and [`deliver`] contains the callback.
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_storyteller_photocraft_PhotoCraftActivity_nativePen(
    _env: *mut c_void,
    _this: *mut c_void,
    action: i32,
    tool: i32,
    buttons: i32,
    pressure: f32,
    tilt: f32,
    orientation: f32,
) {
    let e = RawEvent { action, tool, buttons, pressure, tilt, orientation };
    if let Update::Set(sample) = handle(&e) {
        let guard = SINK.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(sink) = guard.as_ref() {
            deliver(sink.as_ref(), sample);
        }
    }
}
