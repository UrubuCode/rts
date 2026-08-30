//! Primitivos do namespace `rts:dom` para eventos, foco e input.
//!
//! O bridge só cruza dados simples. O `Dom` mantém listeners, bubbling e filas;
//! a fachada TypeScript copia os callbacks antes de os invocar no runtime.

use rts_core::entry::Provided;
use rts_dom::{ListenerOptions, NodeId};

use crate::value::{handle, int, integer, nothing, string, text};

pub const MEMBERS: &[(&str, Provided)] = &[
    ("addListener", add_listener),
    ("addListenerCb", add_listener_cb),
    ("addListenerCbOptions", add_listener_cb_options),
    ("removeListener", remove_listener),
    ("removeListenerCb", remove_listener_cb),
    ("hasListener", has_listener),
    ("dispatchEvent", dispatch_event),
    ("dispatchCollect", dispatch_collect),
    ("dispatchCbAt", dispatch_cb_at),
    ("dispatchCbNode", dispatch_cb_node),
    ("dispatchCbCapture", dispatch_cb_capture),
    ("dispatchCbPassive", dispatch_cb_passive),
    ("pollEvent", poll_event),
    ("pollEventType", poll_event_type),
    ("pushRawEvent", push_raw_event),
    ("pollRawEvent", poll_raw_event),
    ("pollRawEventType", poll_raw_event_type),
    ("pushRawKeyboardEvent", push_raw_keyboard_event),
    ("pollRawKeyboardEvent", poll_raw_keyboard_event),
    ("rawKeyboardKey", raw_keyboard_key),
    ("rawKeyboardPressed", raw_keyboard_pressed),
    ("rawKeyboardRepeat", raw_keyboard_repeat),
    ("rawKeyboardCtrl", raw_keyboard_ctrl),
    ("rawKeyboardShift", raw_keyboard_shift),
    ("rawKeyboardAlt", raw_keyboard_alt),
    ("rawKeyboardMeta", raw_keyboard_meta),
    ("rawKeyboardTarget", raw_keyboard_target),
    ("pushRawTextInput", push_raw_text_input),
    ("pushRawCompositionEvent", push_raw_composition_event),
    ("pollRawInputEvent", poll_raw_input_event),
    ("rawInputKind", raw_input_kind),
    ("rawInputText", raw_input_text),
    ("rawInputTarget", raw_input_target),
    ("focusInput", focus_input),
    ("focusedInput", focused_input),
    ("inputValue", input_value),
    ("setInputValue", set_input_value),
    ("inputFeedTextAt", input_feed_text_at),
    ("inputBackspaceAt", input_backspace_at),
    ("inputFeedText", input_feed_text),
    ("inputBackspace", input_backspace),
];

fn node(value: u64) -> Option<NodeId> {
    NodeId::from_abi(integer(value, -1))
}

fn resolved_node(doc: u64, value: u64) -> Option<(u64, NodeId)> {
    let id = node(value)?;
    let h = handle(doc);
    let valid = rts_dom::store::with_dom(h, |d| d.resolve(id).is_some()).unwrap_or(false);
    valid.then_some((h, id))
}

extern "C" fn add_listener(_e: u64, _t: u64, doc: u64, n: u64, event: u64, _c: u64) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return nothing();
    };
    let event = text(event);
    rts_dom::store::with_dom_mut(h, |d| {
        if d.resolve(id).is_some() {
            d.add_event_listener(id, &event);
        }
    });
    nothing()
}

extern "C" fn add_listener_cb(_e: u64, _t: u64, doc: u64, n: u64, event: u64, cb: u64) -> u64 {
    add_listener_cb_options(_e, _t, doc, n, event, cb)
}

/// Registo com flags no sufixo interno `\u{001f}<bits>`: bit 0 capture, bit 1
/// once, bit 2 passive. O nome público do evento continua intacto no DOM.
extern "C" fn add_listener_cb_options(
    _e: u64,
    _t: u64,
    doc: u64,
    n: u64,
    event: u64,
    cb: u64,
) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return nothing();
    };
    let raw_event = text(event);
    let (event, options) = raw_event
        .rsplit_once('\u{001f}')
        .map(|(event, flags)| {
            let bits = flags.parse::<u8>().unwrap_or(0);
            (
                event,
                ListenerOptions {
                    capture: bits & 1 != 0,
                    once: bits & 2 != 0,
                    passive: bits & 4 != 0,
                },
            )
        })
        .unwrap_or((raw_event.as_str(), ListenerOptions::default()));
    let callback = cb as i64;
    rts_dom::store::with_dom_mut(h, |d| {
        d.add_event_listener_cb_with_options(id, event, callback, options);
    });
    nothing()
}

extern "C" fn remove_listener_cb(
    _e: u64,
    _t: u64,
    doc: u64,
    n: u64,
    event: u64,
    cb: u64,
) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return nothing();
    };
    let raw_event = text(event);
    let (event, capture) = raw_event
        .rsplit_once('\u{001f}')
        .map(|(event, flags)| (event, flags.parse::<u8>().unwrap_or(0) & 1 != 0))
        .unwrap_or((raw_event.as_str(), false));
    rts_dom::store::with_dom_mut(h, |d| {
        d.remove_event_listener_cb(id, event, cb as i64, capture);
    });
    nothing()
}

extern "C" fn remove_listener(_e: u64, _t: u64, doc: u64, n: u64, event: u64, _c: u64) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return nothing();
    };
    let event = text(event);
    rts_dom::store::with_dom_mut(h, |d| {
        if d.resolve(id).is_some() {
            d.remove_event_listener(id, &event);
        }
    });
    nothing()
}

extern "C" fn has_listener(_e: u64, _t: u64, doc: u64, n: u64, event: u64, _c: u64) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return int(0);
    };
    let event = text(event);
    let yes = rts_dom::store::with_dom(h, |d| d.has_listener(id, &event)).unwrap_or(false);
    int(yes as i64)
}

extern "C" fn dispatch_event(_e: u64, _t: u64, doc: u64, n: u64, event: u64, bubbles: u64) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return int(0);
    };
    let event = text(event);
    let count = rts_dom::store::with_dom_mut(h, |d| {
        d.dispatch_event(id, &event, integer(bubbles, 0) != 0)
    })
    .unwrap_or(0);
    int(count)
}

extern "C" fn dispatch_collect(
    _e: u64,
    _t: u64,
    doc: u64,
    n: u64,
    event: u64,
    bubbles: u64,
) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return int(0);
    };
    let event = text(event);
    let count = rts_dom::store::with_dom_mut(h, |d| {
        d.dispatch_event_collect(id, &event, integer(bubbles, 0) != 0)
    })
    .unwrap_or(0);
    int(count)
}

extern "C" fn dispatch_cb_at(_e: u64, _t: u64, doc: u64, i: u64, _b: u64, _c: u64) -> u64 {
    let i = integer(i, -1);
    if i < 0 {
        return int(0);
    }
    let cb = rts_dom::store::with_dom(handle(doc), |d| {
        d.last_dispatch_at(i as usize)
            .map(|(_, callback)| callback)
            .unwrap_or(0)
    })
    .unwrap_or(0);
    int(cb)
}

extern "C" fn dispatch_cb_node(_e: u64, _t: u64, doc: u64, i: u64, _b: u64, _c: u64) -> u64 {
    let i = integer(i, -1);
    if i < 0 {
        return int(-1);
    }
    let node = rts_dom::store::with_dom(handle(doc), |d| {
        d.last_dispatch_at(i as usize)
            .map(|(id, _)| id.to_abi())
            .unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(node)
}

extern "C" fn dispatch_cb_capture(_e: u64, _t: u64, doc: u64, i: u64, _b: u64, _c: u64) -> u64 {
    let i = integer(i, -1);
    if i < 0 {
        return int(0);
    }
    let capture = rts_dom::store::with_dom(handle(doc), |d| {
        d.last_dispatch_capture_at(i as usize) as i64
    })
    .unwrap_or(0);
    int(capture)
}

extern "C" fn dispatch_cb_passive(_e: u64, _t: u64, doc: u64, i: u64, _b: u64, _c: u64) -> u64 {
    let i = integer(i, -1);
    if i < 0 {
        return int(0);
    }
    let passive = rts_dom::store::with_dom(handle(doc), |d| {
        d.last_dispatch_passive_at(i as usize) as i64
    })
    .unwrap_or(0);
    int(passive)
}

extern "C" fn poll_event(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let out = rts_dom::store::with_dom_mut(handle(doc), |d| {
        d.poll_event().map(|(id, _)| id.to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}

extern "C" fn poll_event_type(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let out = rts_dom::store::with_dom(handle(doc), |d| d.poll_event_type().to_string())
        .unwrap_or_default();
    string(&out)
}

extern "C" fn push_raw_event(_e: u64, _t: u64, doc: u64, n: u64, event: u64, _c: u64) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return nothing();
    };
    let event = text(event);
    rts_dom::store::with_dom_mut(h, |d| {
        if let Some(idx) = d.resolve(id) {
            d.push_raw_event(idx, &event);
        }
    });
    nothing()
}

extern "C" fn poll_raw_event(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let out = rts_dom::store::with_dom_mut(handle(doc), |d| {
        d.poll_raw_event().map(|(id, _)| id.to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}

extern "C" fn poll_raw_event_type(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let out = rts_dom::store::with_dom(handle(doc), |d| d.poll_raw_event_type().to_string())
        .unwrap_or_default();
    string(&out)
}

extern "C" fn push_raw_keyboard_event(
    _e: u64,
    _t: u64,
    doc: u64,
    key_code: u64,
    flags: u64,
    _c: u64,
) -> u64 {
    // flags: bit 0 pressed, bit 1 repeat, bits 2..5 ctrl/shift/alt/meta.
    let bits = integer(flags, 0);
    rts_dom::store::with_dom_mut(handle(doc), |d| {
        d.push_raw_keyboard_event(
            integer(key_code, -1),
            bits & 1 != 0,
            bits & 2 != 0,
            bits & 4 != 0,
            bits & 8 != 0,
            bits & 16 != 0,
            bits & 32 != 0,
        );
    });
    nothing()
}

fn with_raw_keyboard<F, T>(doc: u64, f: F, default: T) -> T
where
    F: FnOnce(&rts_dom::RawKeyboardEvent) -> T,
    T: Copy,
{
    rts_dom::store::with_dom(handle(doc), |d| {
        d.last_raw_keyboard_event().map(f).unwrap_or(default)
    })
    .unwrap_or(default)
}

extern "C" fn poll_raw_keyboard_event(
    _e: u64,
    _t: u64,
    doc: u64,
    _a: u64,
    _b: u64,
    _c: u64,
) -> u64 {
    let out = rts_dom::store::with_dom_mut(handle(doc), |d| {
        d.poll_raw_keyboard_event()
            .map(|event| d.id_of_idx(event.target).to_abi())
            .unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}

extern "C" fn raw_keyboard_key(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    int(with_raw_keyboard(doc, |event| event.key_code, -1))
}

extern "C" fn raw_keyboard_pressed(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    int(with_raw_keyboard(doc, |event| event.pressed as i64, 0))
}

extern "C" fn raw_keyboard_repeat(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    int(with_raw_keyboard(doc, |event| event.repeat as i64, 0))
}

extern "C" fn raw_keyboard_ctrl(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    int(with_raw_keyboard(doc, |event| event.ctrl_key as i64, 0))
}

extern "C" fn raw_keyboard_shift(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    int(with_raw_keyboard(doc, |event| event.shift_key as i64, 0))
}

extern "C" fn raw_keyboard_alt(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    int(with_raw_keyboard(doc, |event| event.alt_key as i64, 0))
}

extern "C" fn raw_keyboard_meta(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    int(with_raw_keyboard(doc, |event| event.meta_key as i64, 0))
}

extern "C" fn raw_keyboard_target(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.last_raw_keyboard_event()
            .map(|event| d.id_of_idx(event.target).to_abi())
            .unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}
extern "C" fn push_raw_text_input(
    _e: u64,
    _t: u64,
    doc: u64,
    value: u64,
    _b: u64,
    _c: u64,
) -> u64 {
    rts_dom::store::with_dom_mut(handle(doc), |d| d.push_raw_text_input(text(value)));
    nothing()
}
extern "C" fn push_raw_composition_event(
    _e: u64,
    _t: u64,
    doc: u64,
    kind: u64,
    value: u64,
    _c: u64,
) -> u64 {
    rts_dom::store::with_dom_mut(handle(doc), |d| {
        d.push_raw_composition_event(integer(kind, -1), text(value));
    });
    nothing()
}
extern "C" fn poll_raw_input_event(
    _e: u64,
    _t: u64,
    doc: u64,
    _a: u64,
    _b: u64,
    _c: u64,
) -> u64 {
    let out = rts_dom::store::with_dom_mut(handle(doc), |d| {
        d.poll_raw_input_event()
            .map(|event| d.id_of_idx(event.target).to_abi())
            .unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}
fn with_raw_input<F, T>(doc: u64, f: F, default: T) -> T
where
    F: FnOnce(&rts_dom::RawInputEvent) -> T,
    T: Copy,
{
    rts_dom::store::with_dom(handle(doc), |d| {
        d.last_raw_input_event().map(f).unwrap_or(default)
    })
    .unwrap_or(default)
}
extern "C" fn raw_input_kind(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    int(with_raw_input(doc, |event| event.kind, -1))
}
extern "C" fn raw_input_text(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let value = rts_dom::store::with_dom(handle(doc), |d| {
        d.last_raw_input_event().map(|event| event.text.clone()).unwrap_or_default()
    })
    .unwrap_or_default();
    string(&value)
}
extern "C" fn raw_input_target(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.last_raw_input_event()
            .map(|event| d.id_of_idx(event.target).to_abi())
            .unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}
extern "C" fn focus_input(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let target = node(n).and_then(|id| rts_dom::store::with_dom(h, |d| d.resolve(id)).flatten());
    rts_dom::store::with_dom_mut(h, |d| d.focus_input(target));
    nothing()
}

extern "C" fn focused_input(
    _e: u64,
    _t: u64,
    doc: u64,
    _a: u64,
    _b: u64,
    _c: u64,
) -> u64 {
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.focused_input().map(|idx| d.id_of_idx(idx).to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}

extern "C" fn input_value(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return string("");
    };
    let value = rts_dom::store::with_dom(h, |d| {
        d.resolve(id).map(|idx| d.input_value(idx)).unwrap_or_default()
    })
    .unwrap_or_default();
    string(&value)
}

/// `dom.setInputValue(doc, no, texto)` — SUBSTITUI o valor de um campo.
///
/// Distinta do `inputFeedTextAt` ao lado, que ACRESCENTA: aquela e uma tecla a
/// chegar e esta e o programa a decidir o conteudo. `el.value = ""` — limpar
/// depois de submeter — nao se escreve com a outra de forma nenhuma.
extern "C" fn set_input_value(_e: u64, _t: u64, doc: u64, n: u64, value: u64, _c: u64) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return int(0);
    };
    let value = text(value);
    let changed = rts_dom::store::with_dom_mut(h, |d| {
        let Some(idx) = d.resolve(id) else { return false };
        d.set_input_value(idx, &value)
    })
    .unwrap_or(false);
    int(changed as i64)
}
extern "C" fn input_feed_text_at(
    _e: u64,
    _t: u64,
    doc: u64,
    n: u64,
    value: u64,
    _c: u64,
) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return int(0);
    };
    let value = text(value);
    let changed = rts_dom::store::with_dom_mut(h, |d| {
        d.resolve(id).map(|idx| d.input_feed_text_at(idx, &value)).unwrap_or(false)
    })
    .unwrap_or(false);
    int(changed as i64)
}

extern "C" fn input_backspace_at(
    _e: u64,
    _t: u64,
    doc: u64,
    n: u64,
    _b: u64,
    _c: u64,
) -> u64 {
    let Some((h, id)) = resolved_node(doc, n) else {
        return int(0);
    };
    let changed = rts_dom::store::with_dom_mut(h, |d| {
        d.resolve(id).map(|idx| d.input_backspace_at(idx)).unwrap_or(false)
    })
    .unwrap_or(false);
    int(changed as i64)
}

extern "C" fn input_feed_text(_e: u64, _t: u64, doc: u64, value: u64, _b: u64, _c: u64) -> u64 {
    let value = text(value);
    let changed =
        rts_dom::store::with_dom_mut(handle(doc), |d| d.input_feed_text(&value)).unwrap_or(false);
    int(changed as i64)
}

extern "C" fn input_backspace(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let changed =
        rts_dom::store::with_dom_mut(handle(doc), |d| d.input_backspace()).unwrap_or(false);
    int(changed as i64)
}
