use crate::namespaces::{DispatchOutcome, arg_to_u64};
use crate::namespaces::value::JsValue;

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "rts.alloc" => {
            let size = arg_to_u64(args, 0) as usize;
            if size == 0 {
                return Some(DispatchOutcome::Value(JsValue::Number(0.0)));
            }
            let layout = std::alloc::Layout::from_size_align(size, 8).ok()?;
            let ptr = unsafe { std::alloc::alloc_zeroed(layout) } as u64;
            Some(DispatchOutcome::Value(JsValue::Number(ptr as f64)))
        }
        "rts.free" => {
            let ptr = arg_to_u64(args, 0) as usize;
            let size = arg_to_u64(args, 1) as usize;
            if ptr != 0 && size != 0 {
                let layout = std::alloc::Layout::from_size_align(size, 8).ok()?;
                unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
            }
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "rts.mem_copy" => {
            let dst = arg_to_u64(args, 0) as *mut u8;
            let src = arg_to_u64(args, 1) as *const u8;
            let len = arg_to_u64(args, 2) as usize;
            if !dst.is_null() && !src.is_null() && len > 0 {
                unsafe { std::ptr::copy_nonoverlapping(src, dst, len) };
            }
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "rts.i64_add" => {
            let a = arg_to_u64(args, 0) as i64;
            let b = arg_to_u64(args, 1) as i64;
            Some(DispatchOutcome::Value(JsValue::Number(
                a.wrapping_add(b) as f64,
            )))
        }
        "rts.f64_mul" => {
            let a = args.first().and_then(|v| {
                if let JsValue::Number(n) = v { Some(*n) } else { None }
            }).unwrap_or(0.0);
            let b = args.get(1).and_then(|v| {
                if let JsValue::Number(n) = v { Some(*n) } else { None }
            }).unwrap_or(0.0);
            Some(DispatchOutcome::Value(JsValue::Number(a * b)))
        }
        "rts.str_new" => {
            // ptr + len → handle (para uso futuro com GC)
            let ptr = arg_to_u64(args, 0);
            let _len = arg_to_u64(args, 1);
            Some(DispatchOutcome::Value(JsValue::Number(ptr as f64)))
        }
        _ => None,
    }
}
