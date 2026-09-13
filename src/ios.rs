//! iOS document picker.
//!
//! `UIDocumentPickerViewController` is presented from the app's key window and
//! its delegate is declared in Rust with `objc2`'s `declare_class!`, so no
//! Swift or Objective-C file is needed.
//!
//! Picked files are security-scoped URLs: the delegate keeps the scope open and
//! stores the path, and the caller reads the bytes with `std::fs` once the
//! picker has closed.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{ClassType, DeclaredClass, MainThreadOnly, declare_class, msg_send_id};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString, NSURL};
use objc2_ui_kit::{
    UIApplication, UIDocumentPickerDelegate, UIDocumentPickerMode, UIDocumentPickerViewController,
};

static PICKED: Mutex<Option<PathBuf>> = Mutex::new(None);
static OPEN: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    // `delegate` is a weak property, so the picker does not keep it alive.
    static DELEGATE: RefCell<Option<Retained<PickerDelegate>>> = RefCell::new(None);
}

struct PickerDelegateIvars;

declare_class!(
    struct PickerDelegate;

    unsafe impl ClassType for PickerDelegate {
        type Super = NSObject;
        type Mutability = MainThreadOnly;
        const NAME: &'static str = "RzlPickerDelegate";
    }

    impl DeclaredClass for PickerDelegate {
        type Ivars = PickerDelegateIvars;
    }

    unsafe impl NSObjectProtocol for PickerDelegate {}

    unsafe impl UIDocumentPickerDelegate for PickerDelegate {
        #[method(documentPicker:didPickDocumentsAtURLs:)]
        fn documentPicker_didPickDocumentsAtURLs(
            &self,
            _controller: &UIDocumentPickerViewController,
            urls: &NSArray<NSURL>,
        ) {
            OPEN.store(false, Ordering::SeqCst);
            let Some(url) = urls.firstObject() else {
                return;
            };
            // Keep the security scope open: the bytes are read after this
            // returns, from the game loop.
            let _ = unsafe { url.startAccessingSecurityScopedResource() };
            if let Some(path) = url.path() {
                *PICKED.lock().unwrap() = Some(PathBuf::from(path.to_string()));
            }
        }

        #[method(documentPickerWasCancelled:)]
        fn documentPickerWasCancelled(&self, _controller: &UIDocumentPickerViewController) {
            OPEN.store(false, Ordering::SeqCst);
            CANCELLED.store(true, Ordering::SeqCst);
        }
    }
);

pub fn is_open() -> bool {
    OPEN.load(Ordering::SeqCst)
}

pub fn take_picked() -> Option<PathBuf> {
    PICKED.lock().unwrap().take()
}

pub fn open_picker(uti: &str) {
    CANCELLED.store(false, Ordering::SeqCst);

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    let delegate = mtm
        .alloc::<PickerDelegate>()
        .set_ivars(PickerDelegateIvars);
    let delegate: Retained<PickerDelegate> = unsafe { msg_send_id![delegate, init] };

    let types = NSArray::from_retained_slice(&[NSString::from_str(uti)]);
    #[allow(deprecated)]
    let picker: Retained<UIDocumentPickerViewController> = unsafe {
        UIDocumentPickerViewController::initWithDocumentTypes_inMode(
            mtm.alloc::<UIDocumentPickerViewController>(),
            &types,
            UIDocumentPickerMode::Import,
        )
    };

    unsafe {
        picker.setAllowsMultipleSelection(false);
        picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    }

    let app = UIApplication::sharedApplication(mtm);
    let Some(root) = app
        .windows()
        .firstObject()
        .and_then(|window| window.rootViewController())
    else {
        return;
    };

    unsafe { root.presentViewController_animated_completion(&picker, true, None) };
    OPEN.store(true, Ordering::SeqCst);

    DELEGATE.with(|slot| *slot.borrow_mut() = Some(delegate));
}
