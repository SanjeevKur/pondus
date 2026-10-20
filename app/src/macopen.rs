// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Files that macOS asks Pondus to open: a double click on a .pondus file in Finder, a file dropped
//! on the Dock icon, or `open model.pondus`. macOS sends these as an "open documents" Apple Event,
//! not as a command-line argument, and winit does not pass them on, so Pondus installs its own
//! handler. It is installed when the application is about to finish launching, which is when
//! Cocoa expects it (its own default handler is in place by then and the event not yet sent), and
//! the paths wait in a queue that the app takes each frame.

use std::path::PathBuf;
use std::sync::Mutex;

static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The files asked for since the last call.
pub fn take() -> Vec<PathBuf> {
    PENDING.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::PENDING;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{define_class, msg_send, sel, AllocAnyThread};
    use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter, NSString};

    const fn four_cc(s: &[u8; 4]) -> u32 {
        ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
    }
    const CORE_EVENT_CLASS: u32 = four_cc(b"aevt");
    const OPEN_DOCUMENTS: u32 = four_cc(b"odoc");
    const DIRECT_OBJECT: u32 = four_cc(b"----");

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "PondusOpenDocuments"]
        struct Handler;

        impl Handler {
            #[unsafe(method(handleOpen:withReply:))]
            fn handle_open(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
                let list: Option<Retained<NSAppleEventDescriptor>> = unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
                let Some(list) = list else { return };
                let n = list.numberOfItems();
                let mut paths = vec![];
                if n == 0 {
                    paths.extend(path_of(&list));
                }
                for i in 1..=n {
                    if let Some(item) = list.descriptorAtIndex(i) {
                        paths.extend(path_of(&item));
                    }
                }
                if let Ok(mut v) = PENDING.lock() {
                    v.extend(paths);
                }
            }

            #[unsafe(method(willFinishLaunching:))]
            fn will_finish_launching(&self, _note: &NSNotification) {
                install_handler(self);
            }
        }
    );

    fn path_of(d: &NSAppleEventDescriptor) -> Option<std::path::PathBuf> {
        let url = d.fileURLValue()?;
        url.path().map(|p| std::path::PathBuf::from(p.to_string()))
    }

    fn install_handler(h: &Handler) {
        unsafe {
            let m = NSAppleEventManager::sharedAppleEventManager();
            let _: () = msg_send![&*m, setEventHandler: h, andSelector: sel!(handleOpen:withReply:), forEventClass: CORE_EVENT_CLASS, andEventID: OPEN_DOCUMENTS];
        }
    }

    /// Before the event loop starts: observe the launch, so that the handler is installed just
    /// before Cocoa sends the files it was launched with.
    pub fn install() {
        let h: Retained<Handler> = unsafe { msg_send![Handler::alloc(), init] };
        unsafe {
            let center = NSNotificationCenter::defaultCenter();
            let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
            let obj: &AnyObject = &h;
            center.addObserver_selector_name_object(obj, sel!(willFinishLaunching:), Some(&name), None);
        }
        // installed now too, for a launch that is already past that point
        install_handler(&h);
        // the handler lives for the whole run of the program
        std::mem::forget(h);
    }
}

#[cfg(target_os = "macos")]
pub use imp::install;

#[cfg(not(target_os = "macos"))]
pub fn install() {}
