mod backend;

use crate::namespaces::lang::JsValue;

use super::io;
use super::{arg_to_string, arg_to_u8, arg_to_usize_or_default, DispatchOutcome, NamespaceMember, NamespaceSpec};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "create",
        callee: "window.create",
        doc: "Creates a native window with the given title, width, and height. Returns a handle.",
        ts_signature: "create(title: str, width: u32, height: u32): io.Result<u64>",
    },
    NamespaceMember {
        name: "show",
        callee: "window.show",
        doc: "Shows a window.",
        ts_signature: "show(handle: u64): io.Result<void>",
    },
    NamespaceMember {
        name: "hide",
        callee: "window.hide",
        doc: "Hides a window.",
        ts_signature: "hide(handle: u64): io.Result<void>",
    },
    NamespaceMember {
        name: "close",
        callee: "window.close",
        doc: "Closes and destroys a window.",
        ts_signature: "close(handle: u64): void",
    },
    NamespaceMember {
        name: "set_title",
        callee: "window.set_title",
        doc: "Changes the window title.",
        ts_signature: "set_title(handle: u64, title: str): io.Result<void>",
    },
    NamespaceMember {
        name: "set_size",
        callee: "window.set_size",
        doc: "Resizes a window.",
        ts_signature: "set_size(handle: u64, width: u32, height: u32): io.Result<void>",
    },
    NamespaceMember {
        name: "is_open",
        callee: "window.is_open",
        doc: "Returns true if the window is still open.",
        ts_signature: "is_open(handle: u64): bool",
    },
    NamespaceMember {
        name: "poll_event",
        callee: "window.poll_event",
        doc: "Polls events and updates the window. Returns event string or \"none\".",
        ts_signature: "poll_event(handle: u64): str",
    },
    NamespaceMember {
        name: "fill_rect",
        callee: "window.fill_rect",
        doc: "Fills a rectangle with a color (r,g,b 0-255).",
        ts_signature: "fill_rect(handle: u64, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8): io.Result<void>",
    },
    NamespaceMember {
        name: "draw_text",
        callee: "window.draw_text",
        doc: "Draws text at position (x,y) with a color (bitmap font, no-op until font loaded).",
        ts_signature: "draw_text(handle: u64, text: str, x: i32, y: i32, r: u8, g: u8, b: u8): io.Result<void>",
    },
    NamespaceMember {
        name: "set_pixel",
        callee: "window.set_pixel",
        doc: "Sets a single pixel color.",
        ts_signature: "set_pixel(handle: u64, x: i32, y: i32, r: u8, g: u8, b: u8): io.Result<void>",
    },
    NamespaceMember {
        name: "clear",
        callee: "window.clear",
        doc: "Clears the entire window with a background color.",
        ts_signature: "clear(handle: u64, r: u8, g: u8, b: u8): io.Result<void>",
    },
    NamespaceMember {
        name: "present",
        callee: "window.present",
        doc: "Copies the backbuffer to the window. Call after drawing a frame.",
        ts_signature: "present(handle: u64): io.Result<void>",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "window",
    doc: "Native window management with pixel buffer (cross-platform via minifb).",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "window.create" if args.len() >= 3 => {
            let title = arg_to_string(args, 0);
            let width = arg_to_usize_or_default(args, 1, 800);
            let height = arg_to_usize_or_default(args, 2, 600);
            let result = match backend::create(&title, width, height) {
                Ok(id) => io::result_ok(JsValue::Number(id as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.show" if !args.is_empty() => {
            let result = match backend::show(args[0].to_number() as u64) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.hide" if !args.is_empty() => {
            let result = match backend::hide(args[0].to_number() as u64) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.close" if !args.is_empty() => {
            backend::close(args[0].to_number() as u64);
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "window.set_title" if args.len() >= 2 => {
            let id = args[0].to_number() as u64;
            let title = arg_to_string(args, 1);
            let result = match backend::set_title(id, &title) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.set_size" if args.len() >= 3 => {
            let id = args[0].to_number() as u64;
            let w = arg_to_usize_or_default(args, 1, 800);
            let h = arg_to_usize_or_default(args, 2, 600);
            let result = match backend::set_size(id, w, h) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.is_open" if !args.is_empty() => {
            let open = backend::is_open(args[0].to_number() as u64);
            Some(DispatchOutcome::Value(JsValue::Bool(open)))
        }
        "window.poll_event" if !args.is_empty() => {
            let event = backend::poll_event(args[0].to_number() as u64);
            Some(DispatchOutcome::Value(JsValue::String(event)))
        }
        "window.fill_rect" if args.len() >= 8 => {
            let id = args[0].to_number() as u64;
            let x = arg_to_usize_or_default(args, 1, 0) as i32;
            let y = arg_to_usize_or_default(args, 2, 0) as i32;
            let w = arg_to_usize_or_default(args, 3, 0) as i32;
            let h = arg_to_usize_or_default(args, 4, 0) as i32;
            let r = arg_to_u8(args, 5);
            let g = arg_to_u8(args, 6);
            let b = arg_to_u8(args, 7);
            let result = match backend::fill_rect(id, x, y, w, h, r, g, b) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.draw_text" if args.len() >= 7 => {
            let id = args[0].to_number() as u64;
            let text = arg_to_string(args, 1);
            let x = arg_to_usize_or_default(args, 2, 0) as i32;
            let y = arg_to_usize_or_default(args, 3, 0) as i32;
            let r = arg_to_u8(args, 4);
            let g = arg_to_u8(args, 5);
            let b = arg_to_u8(args, 6);
            let result = match backend::draw_text(id, &text, x, y, r, g, b) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.set_pixel" if args.len() >= 6 => {
            let id = args[0].to_number() as u64;
            let x = arg_to_usize_or_default(args, 1, 0) as i32;
            let y = arg_to_usize_or_default(args, 2, 0) as i32;
            let r = arg_to_u8(args, 3);
            let g = arg_to_u8(args, 4);
            let b = arg_to_u8(args, 5);
            let result = match backend::set_pixel(id, x, y, r, g, b) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.clear" if args.len() >= 4 => {
            let id = args[0].to_number() as u64;
            let r = arg_to_u8(args, 1);
            let g = arg_to_u8(args, 2);
            let b = arg_to_u8(args, 3);
            let result = match backend::clear(id, r, g, b) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "window.present" if !args.is_empty() => {
            let id = args[0].to_number() as u64;
            let result = match backend::present(id) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        _ => None,
    }
}
