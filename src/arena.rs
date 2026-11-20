//! Where MTG Arena is on screen, and whether it is the app in front.
//!
//! macOS only, like [`crate::native`]: these two files are the platform seam. Everything here
//! is in CoreGraphics' global coordinates: points, origin at the top-left of the primary
//! display, y growing downwards.

use std::ffi::c_void;

use core_foundation::base::TCFType;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::display::CGDisplay;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly, kCGWindowOwnerName, kCGWindowOwnerPID,
};
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

/// The name Arena's process goes by, on Steam and from the Epic or direct installers alike.
const OWNER: &str = "MTGA";

/// Arena's main window.
#[derive(Clone, Copy, Debug)]
pub struct Arena {
    pub bounds: CGRect,
    pub pid: i64,
}

/// Arena's window on the current Space, if it has one. The largest ordinary window wins, so
/// a launcher or a stray helper window is never taken for the game.
pub fn window() -> Option<Arena> {
    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let windows = copy_window_info(options, kCGNullWindowID)?;
    let mut best: Option<Arena> = None;
    for item in windows.iter() {
        // SAFETY: CGWindowListCopyWindowInfo returns an array of dictionaries, and `windows`
        // keeps them alive for the loop.
        let info: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        // SAFETY: the keys are CoreGraphics' own constants, and each value has the type the
        // window-list documentation gives for its key.
        let (owner, layer, pid, bounds) = unsafe {
            let get = |key: CFStringRef| info.find(key as *const c_void).map(|value| *value);
            let Some(owner) = get(kCGWindowOwnerName) else { continue };
            let number = |key| get(key).and_then(|n| CFNumber::wrap_under_get_rule(n as CFNumberRef).to_i64());
            let bounds = get(kCGWindowBounds).and_then(|b| {
                CGRect::from_dict_representation(&CFDictionary::wrap_under_get_rule(b as CFDictionaryRef))
            });
            (
                CFString::wrap_under_get_rule(owner as CFStringRef).to_string(),
                number(kCGWindowLayer),
                number(kCGWindowOwnerPID),
                bounds,
            )
        };
        let (Some(bounds), Some(pid)) = (bounds, pid) else { continue };
        if owner != OWNER || layer != Some(0) || bounds.size.width <= 600. || bounds.size.height <= 400. {
            continue;
        }
        let area = |r: &CGRect| r.size.width * r.size.height;
        if best.is_none_or(|b| area(&bounds) > area(&b.bounds)) {
            best = Some(Arena { bounds, pid });
        }
    }
    best
}

/// The process id of the app in front. Call on the main thread.
pub fn frontmost_pid() -> Option<i64> {
    // SAFETY: plain AppKit getters; a nil app is checked before it is messaged.
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        Some(pid.into())
    }
}

/// The display most of `rect` is on, falling back to the primary one.
pub fn display_for(rect: &CGRect) -> CGRect {
    let overlap = |d: &CGRect| {
        let w = (rect.origin.x + rect.size.width).min(d.origin.x + d.size.width) - rect.origin.x.max(d.origin.x);
        let h = (rect.origin.y + rect.size.height).min(d.origin.y + d.size.height) - rect.origin.y.max(d.origin.y);
        w.max(0.) * h.max(0.)
    };
    CGDisplay::active_displays()
        .unwrap_or_default()
        .into_iter()
        .map(|id| CGDisplay::new(id).bounds())
        .filter(|d| overlap(d) > 0.)
        .max_by(|a, b| overlap(a).total_cmp(&overlap(b)))
        .unwrap_or_else(primary_display)
}

/// The display with the menu bar: the one both coordinate systems are measured from.
pub fn primary_display() -> CGRect {
    CGDisplay::main().bounds()
}

/// A stand-in window for trying layouts without the game: `size` at the top-left of the
/// primary display.
pub fn fake(width: f64, height: f64) -> Arena {
    let origin = primary_display().origin;
    Arena { bounds: CGRect::new(&CGPoint::new(origin.x, origin.y), &CGSize::new(width, height)), pid: 0 }
}
