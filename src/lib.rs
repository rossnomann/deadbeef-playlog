#![allow(clippy::missing_safety_doc)]
use ffix::string::expose_string;
use libc::{malloc, memset};
use reqwest::blocking::Client;
use std::{
    mem::size_of,
    ptr::null_mut,
    sync::mpsc::{channel, Sender},
    thread::{self, JoinHandle},
};

mod api;
mod event;
mod publisher;
mod sys;

use self::{
    api::Api,
    event::Event,
    publisher::{Payload, Publisher},
    sys::{DB_functions_t, DB_plugin_t, DB_API_VERSION_MAJOR, DB_API_VERSION_MINOR, DB_PLUGIN_MISC},
};

const PLUGIN_VERSION_MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
const PLUGIN_VERSION_MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
const PLUGIN_ID: &str = "playlog";
const PLUGIN_NAME: &str = "Playlog";
const PLUGIN_DESCRIPTION: &str = r#"Sends played songs information to an HTTP server"#;
const PLUGIN_COPYRIGHT: &str = env!("CARGO_PKG_AUTHORS");
const PLUGIN_WEBSITE: &str = "https://github.com/rossnomann/deadbeef-playlog";
const PLUGIN_CONFIGDIALOG: &str = r#"property URL entry playlog.url "";
property Secret entry playlog.secret "";"#;

static mut CONTEXT: Option<Context> = None;

struct Context {
    api: Api,
    sender: Sender<Payload>,
    publisher: JoinHandle<()>,
}

#[no_mangle]
pub unsafe extern "C" fn playlog_load(api: *mut DB_functions_t) -> *mut DB_plugin_t {
    macro_rules! abort {
        ($e:expr, $msg:expr) => {{
            match $e {
                Ok(val) => val,
                Err(err) => {
                    eprintln!("[playlog] {}: {}", $msg, err);
                    return null_mut();
                }
            }
        }};
        ($e:expr) => {
            abort!($e, "An error has occurred")
        };
    }

    let (tx, rx) = channel();
    let api = abort!(Api::new(api));
    let url = abort!(api.conf_get_str("playlog.url"), "Failed to get url");
    let secret = abort!(api.conf_get_str("playlog.secret"), "Failed to get secret");
    let publisher = abort!(Publisher::new(Client::new(), url, secret.as_bytes(), rx));

    let raw_ptr = {
        let size = size_of::<DB_plugin_t>();
        let ptr = malloc(size);
        assert!(!ptr.is_null());
        memset(ptr, 0, size);
        ptr as *mut DB_plugin_t
    };
    let raw = &mut *raw_ptr;
    raw.type_ = DB_PLUGIN_MISC as i32;
    raw.api_vmajor = DB_API_VERSION_MAJOR as i16;
    raw.api_vminor = DB_API_VERSION_MINOR as i16;
    raw.version_major = abort!(PLUGIN_VERSION_MAJOR.parse(), "Can not parse major plugin version");
    raw.version_minor = abort!(PLUGIN_VERSION_MINOR.parse(), "Can not parse minor plugin version");
    raw.flags = 0;
    raw.reserved1 = 0;
    raw.reserved2 = 0;
    raw.reserved3 = 0;
    raw.id = abort!(expose_string(PLUGIN_ID), "Failed to set plugin ID");
    raw.name = abort!(expose_string(PLUGIN_NAME), "Failed to set plugin name");
    raw.descr = abort!(expose_string(PLUGIN_DESCRIPTION), "Failed to set plugin description");
    raw.copyright = abort!(expose_string(PLUGIN_COPYRIGHT), "Failed to set plugin copyright");
    raw.website = abort!(expose_string(PLUGIN_WEBSITE), "Failed to set plugin website");
    raw.configdialog = abort!(expose_string(PLUGIN_CONFIGDIALOG), "Failed to set plugin configdialog");
    raw.command = None;
    raw.exec_cmdline = None;
    raw.start = Some(on_start);
    raw.stop = Some(on_stop);
    raw.connect = Some(on_connect);
    raw.disconnect = Some(on_disconnect);
    raw.get_actions = None;
    raw.message = Some(on_message);

    let publisher = thread::spawn(move || publisher.run());
    CONTEXT = Some(Context {
        api,
        sender: tx,
        publisher,
    });

    raw_ptr
}

unsafe extern "C" fn on_start() -> i32 {
    0
}

unsafe extern "C" fn on_stop() -> i32 {
    let context = match CONTEXT.take() {
        Some(context) => context,
        None => {
            eprintln!("[playlog] Failed to get context");
            return 0;
        }
    };
    if let Err(err) = context.sender.send(Payload::Stop) {
        eprintln!("[playlog] can not send event: {}", err);
    }
    if let Err(err) = context.publisher.join() {
        eprintln!(
            "[playlog] an error has occurred when joining a publisher thread: {:?}",
            err
        );
    }
    0
}

unsafe extern "C" fn on_connect() -> i32 {
    0
}

unsafe extern "C" fn on_disconnect() -> i32 {
    0
}

unsafe extern "C" fn on_message(id: u32, ctx: usize, p1: u32, p2: u32) -> i32 {
    let context = match CONTEXT {
        Some(ref context) => context,
        None => {
            eprintln!("[playlog] Failed to get context");
            return 0;
        }
    };
    match Event::from_raw(context.api, id, ctx, p1, p2) {
        Ok(Some(event)) => {
            if let Err(err) = context.sender.send(Payload::Event(event)) {
                eprintln!("[playlog] can not send event: {}", err);
            }
        }
        Ok(None) => { /* noop */ }
        Err(err) => eprintln!("[playlog] An error has occurred when handling event: {}", err),
    }
    0
}
