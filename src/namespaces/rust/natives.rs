/// Extensões de coerção de tipos mistos.
///
/// O HIR injeta chamadas a `rts.natives.*` quando detecta operandos de tipos
/// incompatíveis em tempo de compilação. Sem estado — operações puras.
use crate::namespaces::value::JsValue;
use crate::namespaces::{DispatchOutcome, arg_to_value};

fn coerce_to_number(v: &JsValue) -> f64 {
    v.to_number()
}

fn coerce_to_string(v: &JsValue) -> String {
    v.to_js_string()
}

fn coerce_to_bool(v: &JsValue) -> bool {
    v.truthy()
}

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "rts.natives.to_string" => {
            let val = arg_to_value(args, 0);
            Some(DispatchOutcome::Value(JsValue::String(coerce_to_string(&val))))
        }
        "rts.natives.to_number" => {
            let val = arg_to_value(args, 0);
            Some(DispatchOutcome::Value(JsValue::Number(coerce_to_number(&val))))
        }
        "rts.natives.to_bool" => {
            let val = arg_to_value(args, 0);
            Some(DispatchOutcome::Value(JsValue::Bool(coerce_to_bool(&val))))
        }
        "rts.natives.merge" => {
            // Merge genérico: string + qualquer → string, número + número → número
            let a = arg_to_value(args, 0);
            let b = arg_to_value(args, 1);
            let result = match (&a, &b) {
                (JsValue::String(_), _) | (_, JsValue::String(_)) => {
                    JsValue::String(format!("{}{}", coerce_to_string(&a), coerce_to_string(&b)))
                }
                _ => JsValue::Number(coerce_to_number(&a) + coerce_to_number(&b)),
            };
            Some(DispatchOutcome::Value(result))
        }
        "rts.natives.add_mixed" => {
            let a = arg_to_value(args, 0);
            let b = arg_to_value(args, 1);
            // Segue semântica JS: string + qualquer → concatenação
            let result = match (&a, &b) {
                (JsValue::String(_), _) | (_, JsValue::String(_)) => {
                    JsValue::String(format!("{}{}", coerce_to_string(&a), coerce_to_string(&b)))
                }
                _ => JsValue::Number(coerce_to_number(&a) + coerce_to_number(&b)),
            };
            Some(DispatchOutcome::Value(result))
        }
        "rts.natives.eq_loose" => {
            let a = arg_to_value(args, 0);
            let b = arg_to_value(args, 1);
            let eq = loose_eq(&a, &b);
            Some(DispatchOutcome::Value(JsValue::Bool(eq)))
        }
        "rts.natives.compare" => {
            let a = arg_to_value(args, 0);
            let b = arg_to_value(args, 1);
            let result = match (&a, &b) {
                (JsValue::String(sa), JsValue::String(sb)) => sa.cmp(sb) as i8 as f64,
                _ => {
                    let na = coerce_to_number(&a);
                    let nb = coerce_to_number(&b);
                    if na < nb { -1.0 } else if na > nb { 1.0 } else { 0.0 }
                }
            };
            Some(DispatchOutcome::Value(JsValue::Number(result)))
        }
        _ => None,
    }
}

/// Igualdade fraca JS (`==`) com coerção.
fn loose_eq(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Null, JsValue::Null)
        | (JsValue::Undefined, JsValue::Undefined)
        | (JsValue::Null, JsValue::Undefined)
        | (JsValue::Undefined, JsValue::Null) => true,
        (JsValue::Number(na), JsValue::Number(nb)) => na == nb,
        (JsValue::String(sa), JsValue::String(sb)) => sa == sb,
        (JsValue::Bool(ba), JsValue::Bool(bb)) => ba == bb,
        // número == string → coerce string para número
        (JsValue::Number(n), JsValue::String(s)) => {
            *n == s.trim().parse::<f64>().unwrap_or(f64::NAN)
        }
        (JsValue::String(s), JsValue::Number(n)) => {
            s.trim().parse::<f64>().unwrap_or(f64::NAN) == *n
        }
        // bool == qualquer → coerce bool para número
        (JsValue::Bool(b), other) => {
            let n = if *b { 1.0 } else { 0.0 };
            loose_eq(&JsValue::Number(n), other)
        }
        (other, JsValue::Bool(b)) => {
            let n = if *b { 1.0 } else { 0.0 };
            loose_eq(other, &JsValue::Number(n))
        }
        _ => false,
    }
}
