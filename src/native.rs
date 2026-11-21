//! The overlay's `NSPanel`, for the things GPUI has no API for: moving a window, and showing
//! and hiding it without closing it.
//!
//! GPUI's `WindowKind::PopUp` already makes the panel what an overlay needs: non-activating,
//! above ordinary windows, on every Space including a fullscreen app's. This adds the rest,
//! and takes away the title bar GPUI gives every window, hidden or not.
//! macOS only, like [`crate::arena`]. Everything here must run on the main thread.

use core_graphics::geometry::CGRect;
use gpui_kit::Window;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::arena;

// NSWindowStyleMask. Borderless is no bits at all.
const NONACTIVATING_PANEL: usize = 1 << 7;

// NSWindowCollectionBehavior: what GPUI sets for a pop-up, plus `ignoresCycle` so ⌘` in
// another app never lands on the panel.
const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
const IGNORES_CYCLE: usize = 1 << 6;
const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

/// A pointer to a window GPUI keeps for the life of the app: free to copy, main thread only.
#[derive(Clone, Copy)]
pub struct Panel(*mut AnyObject);

impl Panel {
    /// The panel behind a GPUI window, set up to stay on screen while another app is active.
    pub fn of(window: &Window) -> Option<Self> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
        let view: *mut AnyObject = handle.ns_view.as_ptr().cast();
        // SAFETY: `view` is the window's live content view, and this runs on the main thread.
        unsafe {
            let panel: *mut AnyObject = msg_send![view, window];
            if panel.is_null() {
                return None;
            }
            // GPUI makes every window a titled one with the title bar hidden, and macOS rounds
            // a titled window's corners itself. A collapsed panel is a tab with square corners
            // against the screen's edge, so the panel draws its own shape in a bare window.
            let _: () = msg_send![panel, setStyleMask: NONACTIVATING_PANEL];
            // The new frame comes with a background, so make the window see-through again, the
            // way GPUI did: a clear colour proper breaks the shadow.
            let clear: *mut AnyObject =
                msg_send![class!(NSColor), colorWithSRGBRed: 0f64, green: 0f64, blue: 0f64, alpha: 0.0001f64];
            let _: () = msg_send![panel, setOpaque: false];
            let _: () = msg_send![panel, setBackgroundColor: clear];
            // A panel hides when its app is not the active one, by default. This app never is.
            let _: () = msg_send![panel, setHidesOnDeactivate: false];
            let behavior = CAN_JOIN_ALL_SPACES | FULL_SCREEN_AUXILIARY | IGNORES_CYCLE;
            let _: () = msg_send![panel, setCollectionBehavior: behavior];
            let _: () = msg_send![panel, setHasShadow: true];
            Some(Self(panel))
        }
    }

    /// Move and resize the panel to `frame`, given in CoreGraphics' global coordinates (see
    /// [`crate::arena`]). AppKit measures from the bottom-left of the primary display instead.
    pub fn place(&self, frame: CGRect) {
        let primary = arena::primary_display();
        let rect = NSRect::new(
            NSPoint::new(frame.origin.x, primary.size.height - (frame.origin.y + frame.size.height)),
            NSSize::new(frame.size.width, frame.size.height),
        );
        // SAFETY: a live window, on the main thread.
        unsafe {
            let current: NSRect = msg_send![self.0, frame];
            if current != rect {
                let _: () = msg_send![self.0, setFrame: rect, display: true];
            }
        }
    }

    /// Put the panel on screen without making this app the active one.
    pub fn show(&self) {
        if !self.is_visible() {
            // SAFETY: as above.
            unsafe {
                let _: () = msg_send![self.0, orderFrontRegardless];
            }
        }
    }

    pub fn hide(&self) {
        if self.is_visible() {
            // SAFETY: as above.
            unsafe {
                let _: () = msg_send![self.0, orderOut: std::ptr::null_mut::<AnyObject>()];
            }
        }
    }

    /// Let clicks and the pointer through to whatever is underneath.
    pub fn ignore_mouse(&self) {
        // SAFETY: as above.
        unsafe {
            let _: () = msg_send![self.0, setIgnoresMouseEvents: true];
        }
    }

    pub fn is_visible(&self) -> bool {
        // SAFETY: as above.
        unsafe { msg_send![self.0, isVisible] }
    }
}
