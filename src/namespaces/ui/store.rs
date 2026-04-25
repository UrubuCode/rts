//! Per-thread handle table for FLTK UI objects.
//!
//! FLTK requires all widget operations on the thread that created the App.
//! thread_local! avoids the need for Send+Sync on FLTK widget types.

use std::cell::RefCell;

use fltk::{
    button, frame, input, menu, misc, output,
    prelude::{WidgetBase, WidgetExt},
    text, valuator, window,
};

const GEN_SHIFT: u32 = 48;
const SLOT_MASK: u64 = (1u64 << GEN_SHIFT) - 1;

pub enum UiEntry {
    App,
    Window(window::Window),
    Button(button::Button),
    Frame(frame::Frame),
    CheckButton(button::CheckButton),
    RadioButton(button::RadioButton),
    Input(input::Input),
    Output(output::Output),
    Slider(valuator::HorSlider),
    Progress(misc::Progress),
    Spinner(misc::Spinner),
    MenuBar(menu::MenuBar),
    TextBuffer(text::TextBuffer),
    TextDisplay(text::TextDisplay),
    TextEditor(text::TextEditor),
    Free,
}

struct UiSlot {
    generation: u16,
    entry: UiEntry,
}

#[derive(Default)]
pub struct UiStore {
    slots: Vec<UiSlot>,
    free_list: Vec<u32>,
}

impl UiStore {
    pub fn alloc(&mut self, entry: UiEntry) -> u64 {
        if let Some(idx) = self.free_list.pop() {
            let slot = &mut self.slots[idx as usize];
            slot.generation = slot.generation.wrapping_add(1);
            slot.entry = entry;
            return encode(slot.generation, idx);
        }
        let idx = self.slots.len() as u32;
        self.slots.push(UiSlot { generation: 1, entry });
        encode(1, idx)
    }

    pub fn free_entry(&mut self, handle: u64) -> bool {
        let Some((expected, idx)) = decode(handle) else { return false; };
        let Some(slot) = self.slots.get_mut(idx as usize) else { return false; };
        if slot.generation != expected { return false; }
        slot.entry = UiEntry::Free;
        self.free_list.push(idx);
        true
    }

    pub fn get(&self, handle: u64) -> Option<&UiEntry> {
        let (expected, idx) = decode(handle)?;
        let slot = self.slots.get(idx as usize)?;
        if slot.generation != expected || matches!(slot.entry, UiEntry::Free) {
            return None;
        }
        Some(&slot.entry)
    }

    pub fn get_mut(&mut self, handle: u64) -> Option<&mut UiEntry> {
        let (expected, idx) = decode(handle)?;
        let slot = self.slots.get_mut(idx as usize)?;
        if slot.generation != expected || matches!(slot.entry, UiEntry::Free) {
            return None;
        }
        Some(&mut slot.entry)
    }
}

fn encode(generation: u16, slot: u32) -> u64 {
    ((generation as u64) << GEN_SHIFT) | (slot as u64 & SLOT_MASK)
}

fn decode(handle: u64) -> Option<(u16, u32)> {
    if handle == 0 { return None; }
    let generation = ((handle >> GEN_SHIFT) & 0xFFFF) as u16;
    let slot = (handle & SLOT_MASK) as u32;
    Some((generation, slot))
}

thread_local! {
    static UI_STORE: RefCell<UiStore> = RefCell::new(UiStore::default());
}

pub fn alloc_entry(entry: UiEntry) -> u64 {
    UI_STORE.with(|s| s.borrow_mut().alloc(entry))
}

pub fn free_entry(handle: u64) -> bool {
    UI_STORE.with(|s| s.borrow_mut().free_entry(handle))
}

pub fn with_entry<F, R>(handle: u64, f: F) -> Option<R>
where
    F: FnOnce(&UiEntry) -> R,
{
    UI_STORE.with(|s| {
        let store = s.borrow();
        store.get(handle).map(f)
    })
}

pub fn with_entry_mut<F, R>(handle: u64, f: F) -> Option<R>
where
    F: FnOnce(&mut UiEntry) -> R,
{
    UI_STORE.with(|s| {
        let mut store = s.borrow_mut();
        store.get_mut(handle).map(f)
    })
}

/// Clone a TextBuffer without keeping a long-lived borrow of the store.
pub fn clone_textbuf(handle: u64) -> Option<text::TextBuffer> {
    UI_STORE.with(|s| {
        let store = s.borrow();
        match store.get(handle)? {
            UiEntry::TextBuffer(b) => Some(b.clone()),
            _ => None,
        }
    })
}

// ── Generic WidgetExt dispatch ────────────────────────────────────────────
// WidgetExt IS object-safe (set_callback/draw have where Self: Sized so they
// are excluded from the vtable). We can use dyn WidgetExt for the common ops.

macro_rules! dispatch_widget_ext {
    ($entry:expr, $op:expr) => {
        match $entry {
            UiEntry::Window(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::Button(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::Frame(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::CheckButton(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::RadioButton(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::Input(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::Output(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::Slider(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::Progress(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::Spinner(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::MenuBar(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::TextDisplay(w) => $op(w as &mut dyn WidgetExt),
            UiEntry::TextEditor(w) => $op(w as &mut dyn WidgetExt),
            _ => {}
        }
    };
}

pub fn apply_widget_op(handle: u64, op: impl FnOnce(&mut dyn WidgetExt)) {
    UI_STORE.with(|s| {
        let mut store = s.borrow_mut();
        let Some(entry) = store.get_mut(handle) else { return; };
        dispatch_widget_ext!(entry, op);
    });
}

/// Invoke fn_ptr as extern "C" fn() — used for FLTK callbacks.
#[inline(always)]
pub unsafe fn call_fn_ptr(fn_ptr: i64) {
    if fn_ptr != 0 {
        let f: unsafe extern "C" fn() = unsafe { std::mem::transmute(fn_ptr as usize) };
        unsafe { f() };
    }
}

/// Variante com userdata: invoca `extern "C" fn(u64)` passando o
/// handle capturado. Usado por callbacks de classe que capturam `this`.
#[inline(always)]
pub unsafe fn call_fn_ptr_with_ud(fn_ptr: i64, userdata: u64) {
    if fn_ptr != 0 {
        let f: unsafe extern "C" fn(u64) = unsafe { std::mem::transmute(fn_ptr as usize) };
        unsafe { f(userdata) };
    }
}

/// set_callback requires concrete types (where Self: Sized, not in vtable).
macro_rules! dispatch_set_callback {
    ($entry:expr, $fn_ptr:expr) => {{
        let fp = $fn_ptr;
        match $entry {
            UiEntry::Window(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::Button(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::Frame(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::CheckButton(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::RadioButton(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::Input(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::Slider(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::Spinner(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::MenuBar(w) => w.set_callback(move |_| unsafe { call_fn_ptr(fp) }),
            _ => {}
        }
    }};
}

/// Igual a dispatch_set_callback mas captura userdata e passa para o
/// fn_ptr no momento do callback.
macro_rules! dispatch_set_callback_with_ud {
    ($entry:expr, $fn_ptr:expr, $userdata:expr) => {{
        let fp = $fn_ptr;
        let ud = $userdata;
        match $entry {
            UiEntry::Window(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            UiEntry::Button(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            UiEntry::Frame(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            UiEntry::CheckButton(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            UiEntry::RadioButton(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            UiEntry::Input(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            UiEntry::Slider(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            UiEntry::Spinner(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            UiEntry::MenuBar(w) => w.set_callback(move |_| unsafe { call_fn_ptr_with_ud(fp, ud) }),
            _ => {}
        }
    }};
}

/// draw() requires concrete types (where Self: Sized, not in vtable).
macro_rules! dispatch_set_draw {
    ($entry:expr, $fn_ptr:expr) => {{
        let fp = $fn_ptr;
        match $entry {
            UiEntry::Frame(w) => w.draw(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::Window(w) => w.draw(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::Button(w) => w.draw(move |_| unsafe { call_fn_ptr(fp) }),
            UiEntry::CheckButton(w) => w.draw(move |_| unsafe { call_fn_ptr(fp) }),
            _ => {}
        }
    }};
}

pub fn apply_set_callback(handle: u64, fn_ptr: i64) {
    UI_STORE.with(|s| {
        let mut store = s.borrow_mut();
        let Some(entry) = store.get_mut(handle) else { return; };
        dispatch_set_callback!(entry, fn_ptr);
    });
}

pub fn apply_set_callback_with_ud(handle: u64, fn_ptr: i64, userdata: u64) {
    UI_STORE.with(|s| {
        let mut store = s.borrow_mut();
        let Some(entry) = store.get_mut(handle) else { return; };
        dispatch_set_callback_with_ud!(entry, fn_ptr, userdata);
    });
}

pub fn apply_set_draw(handle: u64, fn_ptr: i64) {
    UI_STORE.with(|s| {
        let mut store = s.borrow_mut();
        let Some(entry) = store.get_mut(handle) else { return; };
        dispatch_set_draw!(entry, fn_ptr);
    });
}
