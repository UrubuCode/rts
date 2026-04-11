/// Extensões de coerção de tipos mistos.
///
/// O HIR injeta chamadas a `rts.natives.*` quando detecta operandos de tipos
/// incompatíveis em tempo de compilação. Sem estado — operações puras.
use crate::namespaces::value::RuntimeValue;
use crate::namespaces::{DispatchOutcome, arg_to_value};

fn coerce_to_number(v: &RuntimeValue) -> f64 {
    v.to_number()
}

fn coerce_to_string(v: &RuntimeValue) -> String {
    v.to_js_string()
}

fn coerce_to_bool(v: &RuntimeValue) -> bool {
    v.truthy()
}

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "rts.natives.to_string" => {
            let val = arg_to_value(args, 0);
            Some(DispatchOutcome::Value(RuntimeValue::String(
                coerce_to_string(&val),
            )))
        }
        "rts.natives.to_number" => {
            let val = arg_to_value(args, 0);
            Some(DispatchOutcome::Value(RuntimeValue::Number(
                coerce_to_number(&val),
            )))
        }
        "rts.natives.to_bool" => {
            let val = arg_to_value(args, 0);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(coerce_to_bool(
                &val,
            ))))
        }
        "rts.natives.merge" => {
            // Merge genérico: string + qualquer → string, número + número → número
            let a = arg_to_value(args, 0);
            let b = arg_to_value(args, 1);
            let result = match (&a, &b) {
                (RuntimeValue::String(_), _) | (_, RuntimeValue::String(_)) => {
                    RuntimeValue::String(format!(
                        "{}{}",
                        coerce_to_string(&a),
                        coerce_to_string(&b)
                    ))
                }
                _ => RuntimeValue::Number(coerce_to_number(&a) + coerce_to_number(&b)),
            };
            Some(DispatchOutcome::Value(result))
        }
        "rts.natives.add_mixed" => {
            let a = arg_to_value(args, 0);
            let b = arg_to_value(args, 1);
            // Segue semântica JS: string + qualquer → concatenação
            let result = match (&a, &b) {
                (RuntimeValue::String(_), _) | (_, RuntimeValue::String(_)) => {
                    RuntimeValue::String(format!(
                        "{}{}",
                        coerce_to_string(&a),
                        coerce_to_string(&b)
                    ))
                }
                _ => RuntimeValue::Number(coerce_to_number(&a) + coerce_to_number(&b)),
            };
            Some(DispatchOutcome::Value(result))
        }
        "rts.natives.eq_loose" => {
            let a = arg_to_value(args, 0);
            let b = arg_to_value(args, 1);
            let eq = loose_eq(&a, &b);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(eq)))
        }
        "rts.natives.compare" => {
            let a = arg_to_value(args, 0);
            let b = arg_to_value(args, 1);
            let result = match (&a, &b) {
                (RuntimeValue::String(sa), RuntimeValue::String(sb)) => sa.cmp(sb) as i8 as f64,
                _ => {
                    let na = coerce_to_number(&a);
                    let nb = coerce_to_number(&b);
                    if na < nb {
                        -1.0
                    } else if na > nb {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
            Some(DispatchOutcome::Value(RuntimeValue::Number(result)))
        }
        _ => None,
    }
}

/// Igualdade fraca JS (`==`) com coerção.
fn loose_eq(a: &RuntimeValue, b: &RuntimeValue) -> bool {
    match (a, b) {
        (RuntimeValue::Null, RuntimeValue::Null)
        | (RuntimeValue::Undefined, RuntimeValue::Undefined)
        | (RuntimeValue::Null, RuntimeValue::Undefined)
        | (RuntimeValue::Undefined, RuntimeValue::Null) => true,
        (RuntimeValue::Number(na), RuntimeValue::Number(nb)) => na == nb,
        (RuntimeValue::String(sa), RuntimeValue::String(sb)) => sa == sb,
        (RuntimeValue::Bool(ba), RuntimeValue::Bool(bb)) => ba == bb,
        // número == string → coerce string para número
        (RuntimeValue::Number(n), RuntimeValue::String(s)) => {
            *n == s.trim().parse::<f64>().unwrap_or(f64::NAN)
        }
        (RuntimeValue::String(s), RuntimeValue::Number(n)) => {
            s.trim().parse::<f64>().unwrap_or(f64::NAN) == *n
        }
        // bool == qualquer → coerce bool para número
        (RuntimeValue::Bool(b), other) => {
            let n = if *b { 1.0 } else { 0.0 };
            loose_eq(&RuntimeValue::Number(n), other)
        }
        (other, RuntimeValue::Bool(b)) => {
            let n = if *b { 1.0 } else { 0.0 };
            loose_eq(other, &RuntimeValue::Number(n))
        }
        _ => false,
    }
}
