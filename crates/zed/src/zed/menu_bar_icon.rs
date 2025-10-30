use agent_ui::NewThread;
use cocoa::{
    appkit::{NSMenu, NSMenuItem, NSSquareStatusItemLength, NSStatusBar},
    base::{NO, YES, id, nil, selector},
    foundation::{NSAutoreleasePool, NSString},
};
use gpui::{Action, App, AsyncApp};
use objc::{class, msg_send, runtime::Object, sel, sel_impl};
use std::ffi::c_void;
use std::sync::Mutex;
use util::ResultExt;
use zed_actions::{OpenAccountSettings, Quit};

static MENU_BAR_ICON: Mutex<Option<MenuBarIcon>> = Mutex::new(None);

pub struct MenuBarIcon {
    status_item: id,
    delegate: id,
}

unsafe impl Send for MenuBarIcon {}
unsafe impl Sync for MenuBarIcon {}

pub fn initialize_menu_bar_icon(cx: &App) {
    let mut icon = MENU_BAR_ICON.lock().unwrap();
    if icon.is_none() {
        *icon = Some(MenuBarIcon::new(cx));
    }
}

impl MenuBarIcon {
    pub fn new(cx: &App) -> Self {
        unsafe {
            let status_bar = NSStatusBar::systemStatusBar(nil);
            let status_item: id =
                msg_send![status_bar, statusItemWithLength: NSSquareStatusItemLength];

            // Retain the status item so it doesn't get deallocated
            let _: id = msg_send![status_item, retain];

            let button: id = msg_send![status_item, button];

            if button != nil {
                let symbol_name = ns_string("cube.fill");
                let image: id = msg_send![class!(NSImage), imageWithSystemSymbolName:symbol_name accessibilityDescription:nil];

                if image != nil {
                    let _: () = msg_send![image, setTemplate: YES];
                    let _: () = msg_send![button, setImage: image];
                }
            }

            // Create delegate
            let delegate = create_menu_delegate(cx);

            let menu = Self::create_menu(delegate);
            let _: () = msg_send![status_item, setMenu: menu];

            Self {
                status_item,
                delegate,
            }
        }
    }

    unsafe fn create_menu(delegate: id) -> id {
        unsafe {
            let menu = NSMenu::new(nil).autorelease();

            let recent_threads_item = Self::create_menu_item_with_title("No recent threads");
            let _: () = msg_send![recent_threads_item, setEnabled: NO];
            menu.addItem_(recent_threads_item);

            let separator = NSMenuItem::separatorItem(nil);
            menu.addItem_(separator);

            // New Thread - tag 1
            let new_thread_item = Self::create_menu_item_with_title("New Thread");
            let _: () = msg_send![new_thread_item, setTarget: delegate];
            let _: () = msg_send![new_thread_item, setAction: sel!(handleMenuAction:)];
            let _: () = msg_send![new_thread_item, setTag: 1i64];
            menu.addItem_(new_thread_item);

            let separator2 = NSMenuItem::separatorItem(nil);
            menu.addItem_(separator2);

            // Open Settings - tag 2
            let settings_item = Self::create_menu_item_with_title("Open Settings");
            let _: () = msg_send![settings_item, setTarget: delegate];
            let _: () = msg_send![settings_item, setAction: sel!(handleMenuAction:)];
            let _: () = msg_send![settings_item, setTag: 2i64];
            menu.addItem_(settings_item);

            let separator3 = NSMenuItem::separatorItem(nil);
            menu.addItem_(separator3);

            // Quit - tag 3
            let quit_item = Self::create_menu_item_with_title("Quit");
            let _: () = msg_send![quit_item, setTarget: delegate];
            let _: () = msg_send![quit_item, setAction: sel!(handleMenuAction:)];
            let _: () = msg_send![quit_item, setTag: 3i64];
            menu.addItem_(quit_item);

            menu
        }
    }

    unsafe fn create_menu_item_with_title(title: &str) -> id {
        unsafe {
            let title_str = ns_string(title);
            let item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
                title_str,
                selector(""),
                ns_string(""),
            );
            msg_send![item, autorelease]
        }
    }
}

impl Drop for MenuBarIcon {
    fn drop(&mut self) {
        unsafe {
            let status_bar = NSStatusBar::systemStatusBar(nil);
            let _: () = msg_send![status_bar, removeStatusItem: self.status_item];
            let _: () = msg_send![self.delegate, release];
        }
    }
}

unsafe fn ns_string(string: &str) -> id {
    unsafe {
        let ns_str = NSString::alloc(nil).init_str(string);
        msg_send![ns_str, autorelease]
    }
}

// Objective-C delegate for handling menu actions
use objc::declare::ClassDecl;
use objc::runtime::{Class, Sel};

static mut DELEGATE_CLASS: *const Class = std::ptr::null();

#[ctor::ctor]
unsafe fn build_delegate_class() {
    let mut decl = ClassDecl::new("ZedMenuBarDelegate", class!(NSObject)).unwrap();
    decl.add_ivar::<*mut c_void>("async_cx");

    decl.add_method(
        sel!(handleMenuAction:),
        handle_menu_action as extern "C" fn(&Object, Sel, id),
    );

    decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&Object, Sel));

    DELEGATE_CLASS = decl.register();
}

unsafe fn create_menu_delegate(cx: &App) -> id {
    unsafe {
        let delegate: id = msg_send![DELEGATE_CLASS, alloc];
        let delegate: id = msg_send![delegate, init];

        let async_cx = Box::new(cx.to_async());
        let async_cx_ptr = Box::into_raw(async_cx) as *mut c_void;
        (*delegate).set_ivar("async_cx", async_cx_ptr);

        delegate
    }
}

extern "C" fn handle_menu_action(this: &Object, _sel: Sel, sender: id) {
    unsafe {
        let tag: i64 = msg_send![sender, tag];
        let async_cx_ptr: *mut c_void = *this.get_ivar("async_cx");
        let async_cx = &*(async_cx_ptr as *const AsyncApp);

        match tag {
            1 => {
                // New Thread
                async_cx
                    .update(|cx| {
                        cx.dispatch_action(&NewThread);
                    })
                    .log_err();
            }
            2 => {
                // Open Settings
                async_cx
                    .update(|cx| {
                        cx.dispatch_action(&OpenAccountSettings);
                    })
                    .log_err();
            }
            3 => {
                // Quit
                async_cx
                    .update(|cx| {
                        cx.dispatch_action(&Quit);
                    })
                    .log_err();
            }
            _ => {}
        }
    }
}

extern "C" fn dealloc(this: &Object, _sel: Sel) {
    unsafe {
        let async_cx_ptr: *mut c_void = *this.get_ivar("async_cx");
        if !async_cx_ptr.is_null() {
            let _ = Box::from_raw(async_cx_ptr as *mut AsyncApp);
        }
        let _: () = msg_send![super(this, class!(NSObject)), dealloc];
    }
}
