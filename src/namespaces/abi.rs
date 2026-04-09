use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use crate::namespaces::lang::{JsValue, RuntimeContext, evaluate_expression, evaluate_statement};

use super::DispatchOutcome;

const MAX_ARGS: usize = 6;
const UNDEFINED_HANDLE: i64 = 0;
const HANDLE_SLOT_MASK: u64 = u32::MAX as u64;
const MAX_HANDLE_GENERATION: u32 = i32::MAX as u32;
const MAX_OVERLAY_HANDLES: usize = 2_048;
const TARGET_OVERLAY_BYTES: usize = 4 * 1024 * 1024;
const MIN_RECLAIM_AGE: u64 = 2;

#[derive(Debug, Clone)]
struct BindingEntry {
    handle: i64,
    mutable: bool,
}

#[derive(Debug, Clone)]
struct ValueSlot {
    value: JsValue,
    generation: u32,
    pin_count: usize,
    last_touch_epoch: u64,
    approx_bytes: usize,
}

#[derive(Debug, Default)]
struct ValueStore {
    slots: Vec<Option<ValueSlot>>,
    slot_generations: Vec<u32>,
    free_indices: Vec<usize>,
    overlay: VecDeque<i64>,
    bindings: BTreeMap<String, BindingEntry>,
    epoch: u64,
    live_bytes: usize,
}

impl ValueStore {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn bump_epoch(&mut self) -> u64 {
        self.epoch = self.epoch.saturating_add(1);
        self.epoch
    }

    fn allocate_value(&mut self, value: JsValue) -> i64 {
        if matches!(value, JsValue::Undefined) {
            return UNDEFINED_HANDLE;
        }

        let epoch = self.bump_epoch();
        let slot_index = if let Some(index) = self.free_indices.pop() {
            index
        } else {
            self.slots.push(None);
            self.slot_generations.push(0);
            self.slots.len().saturating_sub(1)
        };

        let generation = self.bump_generation(slot_index);
        let approx_bytes = estimate_value_size(&value);
        let handle = encode_handle(slot_index, generation);

        self.live_bytes = self.live_bytes.saturating_add(approx_bytes);
        self.slots[slot_index] = Some(ValueSlot {
            value,
            generation,
            pin_count: 0,
            last_touch_epoch: epoch,
            approx_bytes,
        });
        self.overlay.push_back(handle);
        self.reclaim_if_needed();
        handle
    }

    fn read_value(&mut self, handle: i64) -> JsValue {
        let Some((slot_index, generation)) = decode_handle(handle) else {
            return JsValue::Undefined;
        };

        let epoch = self.bump_epoch();
        let Some(slot) = self.slots.get_mut(slot_index).and_then(Option::as_mut) else {
            return JsValue::Undefined;
        };

        if slot.generation != generation {
            return JsValue::Undefined;
        }

        slot.last_touch_epoch = epoch;
        slot.value.clone()
    }

    fn bind_identifier(&mut self, name: String, handle: i64, mutable: bool) -> i64 {
        let epoch = self.bump_epoch();
        let bound_handle = self.normalize_live_handle(handle, epoch);

        if let Some(existing) = self.bindings.get(&name) {
            if !existing.mutable {
                return existing.handle;
            }
        }

        if let Some(previous) = self.bindings.remove(&name) {
            self.unpin(previous.handle, epoch);
        }

        if bound_handle > UNDEFINED_HANDLE {
            self.pin(bound_handle, epoch);
        }

        self.bindings.insert(
            name,
            BindingEntry {
                handle: bound_handle,
                mutable,
            },
        );
        self.reclaim_if_needed();
        bound_handle
    }

    fn read_identifier(&mut self, name: &str) -> Option<JsValue> {
        let handle = self.bindings.get(name).map(|entry| entry.handle)?;
        Some(self.read_value(handle))
    }

    fn bind_identifier_value(&mut self, name: String, value: JsValue, mutable: bool) -> JsValue {
        let value_handle = self.allocate_value(value);
        let bound_handle = self.bind_identifier(name, value_handle, mutable);
        self.read_value(bound_handle)
    }

    fn write_identifier(&mut self, name: &str, value: JsValue) -> Result<JsValue, String> {
        let previous = self.bindings.get(name).cloned();

        if let Some(entry) = &previous {
            if !entry.mutable {
                return Err(format!(
                    "cannot assign to constant binding '{}'",
                    name.replace('\n', " ")
                ));
            }
        }

        let value_handle = self.allocate_value(value.clone());
        let epoch = self.bump_epoch();

        if let Some(entry) = previous {
            self.unpin(entry.handle, epoch);
            self.pin(value_handle, epoch);
            self.bindings.insert(
                name.to_string(),
                BindingEntry {
                    handle: value_handle,
                    mutable: true,
                },
            );
        } else {
            let _ = self.bind_identifier(name.to_string(), value_handle, true);
        }

        self.reclaim_if_needed();
        Ok(value)
    }

    fn bump_generation(&mut self, slot_index: usize) -> u32 {
        let current = self.slot_generations.get(slot_index).copied().unwrap_or(0);
        let next = if current >= MAX_HANDLE_GENERATION {
            1
        } else {
            current.saturating_add(1)
        };
        if let Some(generation) = self.slot_generations.get_mut(slot_index) {
            *generation = next;
        }
        next
    }

    fn normalize_live_handle(&mut self, handle: i64, epoch: u64) -> i64 {
        if handle <= UNDEFINED_HANDLE {
            return UNDEFINED_HANDLE;
        }

        let Some((slot_index, generation)) = decode_handle(handle) else {
            return UNDEFINED_HANDLE;
        };
        let Some(slot) = self.slots.get_mut(slot_index).and_then(Option::as_mut) else {
            return UNDEFINED_HANDLE;
        };
        if slot.generation != generation {
            return UNDEFINED_HANDLE;
        }

        slot.last_touch_epoch = epoch;
        handle
    }

    fn pin(&mut self, handle: i64, epoch: u64) {
        if let Some((slot_index, generation)) = decode_handle(handle) {
            if let Some(slot) = self.slots.get_mut(slot_index).and_then(Option::as_mut) {
                if slot.generation == generation {
                    slot.pin_count = slot.pin_count.saturating_add(1);
                    slot.last_touch_epoch = epoch;
                }
            }
        }
    }

    fn unpin(&mut self, handle: i64, epoch: u64) {
        if let Some((slot_index, generation)) = decode_handle(handle) {
            if let Some(slot) = self.slots.get_mut(slot_index).and_then(Option::as_mut) {
                if slot.generation == generation {
                    slot.pin_count = slot.pin_count.saturating_sub(1);
                    slot.last_touch_epoch = epoch;
                }
            }
        }
    }

    fn reclaim_if_needed(&mut self) {
        if self.overlay.len() <= MAX_OVERLAY_HANDLES && self.live_bytes <= TARGET_OVERLAY_BYTES {
            return;
        }

        let mut budget = self.overlay.len();
        while budget > 0
            && (self.overlay.len() > MAX_OVERLAY_HANDLES || self.live_bytes > TARGET_OVERLAY_BYTES)
        {
            budget = budget.saturating_sub(1);

            let Some(candidate) = self.overlay.pop_front() else {
                break;
            };
            let Some((slot_index, generation)) = decode_handle(candidate) else {
                continue;
            };
            let Some(slot) = self.slots.get(slot_index).and_then(Option::as_ref) else {
                continue;
            };
            if slot.generation != generation {
                continue;
            }

            let age = self.epoch.saturating_sub(slot.last_touch_epoch);
            let reclaimable = slot.pin_count == 0 && age >= MIN_RECLAIM_AGE;
            if reclaimable {
                self.release_slot(slot_index);
            } else {
                self.overlay.push_back(candidate);
            }
        }
    }

    fn release_slot(&mut self, slot_index: usize) {
        let Some(slot_entry) = self.slots.get_mut(slot_index) else {
            return;
        };
        if let Some(slot) = slot_entry.take() {
            self.live_bytes = self.live_bytes.saturating_sub(slot.approx_bytes);
            self.free_indices.push(slot_index);
        }
    }

    #[cfg(test)]
    fn live_slots(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }
}

thread_local! {
    static VALUE_STORE: RefCell<ValueStore> = RefCell::new(ValueStore::default());
}

fn with_store_mut<R>(callback: impl FnOnce(&mut ValueStore) -> R) -> R {
    VALUE_STORE.with(|store| {
        let mut borrowed = store.borrow_mut();
        callback(&mut borrowed)
    })
}

pub fn reset_thread_state() {
    with_store_mut(ValueStore::reset);
}

fn push_value(value: JsValue) -> i64 {
    with_store_mut(|store| store.allocate_value(value))
}

fn read_value(handle: i64) -> JsValue {
    with_store_mut(|store| store.read_value(handle))
}

fn bind_identifier(name: String, handle: i64, mutable: bool) -> i64 {
    with_store_mut(|store| store.bind_identifier(name, handle, mutable))
}

fn bind_identifier_value(name: String, value: JsValue, mutable: bool) -> JsValue {
    with_store_mut(|store| store.bind_identifier_value(name, value, mutable))
}

fn write_identifier(name: &str, value: JsValue) -> Result<JsValue, String> {
    with_store_mut(|store| store.write_identifier(name, value))
}

fn read_identifier(name: &str) -> Option<JsValue> {
    with_store_mut(|store| store.read_identifier(name))
}

fn encode_handle(slot_index: usize, generation: u32) -> i64 {
    let slot = (slot_index as u64).saturating_add(1);
    let generation = generation as u64;
    let raw = (generation << 32) | slot;
    raw as i64
}

fn decode_handle(handle: i64) -> Option<(usize, u32)> {
    if handle <= UNDEFINED_HANDLE {
        return None;
    }

    let raw = handle as u64;
    let slot = raw & HANDLE_SLOT_MASK;
    if slot == 0 {
        return None;
    }

    let generation = (raw >> 32) as u32;
    if generation == 0 {
        return None;
    }

    Some((slot.saturating_sub(1) as usize, generation))
}

fn estimate_value_size(value: &JsValue) -> usize {
    match value {
        JsValue::Number(_) => 16,
        JsValue::String(text) => 24 + text.len(),
        JsValue::Bool(_) => 8,
        JsValue::Object(map) => {
            32 + map
                .iter()
                .map(|(key, inner)| key.len() + estimate_value_size(inner))
                .sum::<usize>()
        }
        JsValue::NativeFunction(name) => 24 + name.len(),
        JsValue::Null => 8,
        JsValue::Undefined => 0,
    }
}

fn read_utf8(ptr: i64, len: i64) -> Option<String> {
    if ptr <= 0 || len < 0 {
        return None;
    }

    let ptr = ptr as *const u8;
    let len = len as usize;
    let bytes = unsafe {
        // SAFETY: `ptr` and `len` are emitted by RTS codegen as static data payload references.
        std::slice::from_raw_parts(ptr, len)
    };

    std::str::from_utf8(bytes).ok().map(ToString::to_string)
}

fn decode_args(argc: i64, raw: [i64; MAX_ARGS]) -> Vec<JsValue> {
    let count = argc.clamp(0, MAX_ARGS as i64) as usize;
    (0..count).map(|index| read_value(raw[index])).collect()
}

fn dispatch_builtin(callee: &str, args: Vec<JsValue>) -> Result<JsValue, String> {
    if let Some(outcome) = super::dispatch(callee, &args) {
        return match outcome {
            DispatchOutcome::Value(value) => Ok(value),
            DispatchOutcome::Emit(message) => {
                println!("{message}");
                Ok(JsValue::Undefined)
            }
            DispatchOutcome::Panic(message) => Err(message),
        };
    }

    match callee {
        "Number" => Ok(JsValue::Number(
            args.first()
                .cloned()
                .unwrap_or(JsValue::Undefined)
                .to_number(),
        )),
        "String" => Ok(JsValue::String(
            args.first()
                .cloned()
                .unwrap_or(JsValue::Undefined)
                .to_js_string(),
        )),
        "Boolean" => Ok(JsValue::Bool(
            args.first().cloned().unwrap_or(JsValue::Undefined).truthy(),
        )),
        _ => Ok(JsValue::Undefined),
    }
}

struct AbiEvalContext;

impl RuntimeContext for AbiEvalContext {
    fn read_identifier(&self, name: &str) -> Option<JsValue> {
        read_identifier(name)
    }

    fn call_function(&mut self, callee: &str, args: Vec<JsValue>) -> anyhow::Result<JsValue> {
        dispatch_builtin(callee, args).map_err(anyhow::Error::msg)
    }

    fn define_identifier(
        &mut self,
        name: &str,
        value: JsValue,
        mutable: bool,
    ) -> anyhow::Result<JsValue> {
        Ok(bind_identifier_value(name.to_string(), value, mutable))
    }

    fn write_identifier(&mut self, name: &str, value: JsValue) -> anyhow::Result<JsValue> {
        write_identifier(name, value).map_err(anyhow::Error::msg)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_call_dispatch(
    callee_ptr: i64,
    callee_len: i64,
    argc: i64,
    a0: i64,
    a1: i64,
    a2: i64,
    a3: i64,
    a4: i64,
    a5: i64,
) -> i64 {
    let Some(callee) = read_utf8(callee_ptr, callee_len) else {
        return 0;
    };

    let args = decode_args(argc, [a0, a1, a2, a3, a4, a5]);
    match dispatch_builtin(&callee, args) {
        Ok(value) => push_value(value),
        Err(message) => {
            eprintln!("RTS runtime panic: {message}");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_bind_identifier(
    name_ptr: i64,
    name_len: i64,
    value_handle: i64,
    mutable_flag: i64,
) -> i64 {
    let Some(name) = read_utf8(name_ptr, name_len) else {
        return UNDEFINED_HANDLE;
    };

    bind_identifier(name, value_handle, mutable_flag != 0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_eval_expr(expr_ptr: i64, expr_len: i64) -> i64 {
    let Some(expression) = read_utf8(expr_ptr, expr_len) else {
        return UNDEFINED_HANDLE;
    };

    let mut runtime = AbiEvalContext;
    let value =
        evaluate_expression(&expression, &mut runtime).unwrap_or_else(|_| JsValue::Undefined);
    push_value(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_eval_stmt(stmt_ptr: i64, stmt_len: i64) -> i64 {
    let Some(statement) = read_utf8(stmt_ptr, stmt_len) else {
        return UNDEFINED_HANDLE;
    };

    let mut runtime = AbiEvalContext;
    let value =
        evaluate_statement(&statement, &mut runtime).unwrap_or_else(|_| JsValue::Undefined);
    push_value(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_read_identifier(name_ptr: i64, name_len: i64) -> i64 {
    let Some(name) = read_utf8(name_ptr, name_len) else {
        return UNDEFINED_HANDLE;
    };

    match read_identifier(&name) {
        Some(value) => push_value(value),
        None => UNDEFINED_HANDLE,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_binop(op: i64, lhs_handle: i64, rhs_handle: i64) -> i64 {
    let lhs = read_value(lhs_handle);
    let rhs = read_value(rhs_handle);

    let result = match op {
        0 => {
            // Add
            if lhs.is_string_like() || rhs.is_string_like() {
                JsValue::String(format!("{}{}", lhs.to_js_string(), rhs.to_js_string()))
            } else {
                JsValue::Number(lhs.to_number() + rhs.to_number())
            }
        }
        1 => JsValue::Number(lhs.to_number() - rhs.to_number()),       // Sub
        2 => JsValue::Number(lhs.to_number() * rhs.to_number()),       // Mul
        3 => JsValue::Number(lhs.to_number() / rhs.to_number()),       // Div
        4 => JsValue::Number(lhs.to_number() % rhs.to_number()),       // Mod
        5 => JsValue::Bool(lhs.to_number() > rhs.to_number()),         // Gt
        6 => JsValue::Bool(lhs.to_number() >= rhs.to_number()),        // Gte
        7 => JsValue::Bool(lhs.to_number() < rhs.to_number()),         // Lt
        8 => JsValue::Bool(lhs.to_number() <= rhs.to_number()),        // Lte
        9 => JsValue::Bool(lhs == rhs),                                 // Eq (===)
        10 => JsValue::Bool(lhs != rhs),                                // Ne (!==)
        11 => {
            // LogicAnd
            if !lhs.truthy() { lhs } else { rhs }
        }
        12 => {
            // LogicOr
            if lhs.truthy() { lhs } else { rhs }
        }
        _ => JsValue::Undefined,
    };

    push_value(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_unbox_number(handle: i64) -> i64 {
    let value = read_value(handle);
    let n = value.to_number();
    i64::from_ne_bytes(n.to_ne_bytes())
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_box_number(bits: i64) -> i64 {
    let n = f64::from_ne_bytes(bits.to_ne_bytes());
    push_value(JsValue::Number(n))
}

#[cfg(test)]
mod tests {
    use crate::namespaces::lang::JsValue;

    use super::{
        MAX_OVERLAY_HANDLES, TARGET_OVERLAY_BYTES, bind_identifier, push_value, read_value,
        reset_thread_state, with_store_mut,
    };

    #[test]
    fn eval_context_can_read_bound_identifier() {
        reset_thread_state();
        let value_handle = push_value(JsValue::Number(42.0));
        let _ = bind_identifier("valor".to_string(), value_handle, false);
        let resolved = super::read_identifier("valor").expect("binding should exist");
        assert_eq!(resolved, JsValue::Number(42.0));
    }

    #[test]
    fn overlay_reclaims_old_unpinned_values() {
        reset_thread_state();
        let allocated = MAX_OVERLAY_HANDLES + 512;
        let mut first_handle = 0i64;

        for index in 0..allocated {
            let handle = push_value(JsValue::Number(index as f64));
            if index == 0 {
                first_handle = handle;
            }
        }

        let live_after_sweep = with_store_mut(|store| store.live_slots());
        assert!(live_after_sweep <= MAX_OVERLAY_HANDLES);
        assert_eq!(read_value(first_handle), JsValue::Undefined);

        let bytes_live = with_store_mut(|store| store.live_bytes);
        assert!(bytes_live <= TARGET_OVERLAY_BYTES || live_after_sweep == MAX_OVERLAY_HANDLES);
    }

    #[test]
    fn eval_statement_handles_if_and_for_control_flow() {
        reset_thread_state();

        let mut runtime = super::AbiEvalContext;
        let result = super::evaluate_statement(
            r#"
            let total = 0;
            if (true) {
                total = 1;
            }
            for (let i = 0; i < 2; i++) {
                total = total + 1;
            }
            total;
        "#,
            &mut runtime,
        )
        .expect("statement evaluator should execute control-flow");

        assert_eq!(result, JsValue::Number(3.0));
    }
}
