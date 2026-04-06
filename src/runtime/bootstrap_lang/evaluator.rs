use anyhow::Result;

use super::ast::{BinaryOp, Expression, Literal, UnaryOp};
use super::value::JsValue;

pub trait RuntimeContext {
    fn read_identifier(&self, name: &str) -> Option<JsValue>;
    fn call_function(&mut self, callee: &str, args: Vec<JsValue>) -> Result<JsValue>;
}

pub fn evaluate(expression: &Expression, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    eval_expression(expression, runtime)
}

const BUILTIN_RUNTIME_NAMESPACES: &[&str] = &[
    "io", "fs", "process", "global", "buffer", "promise", "task", "crypto",
];

fn eval_expression(expression: &Expression, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    match expression {
        Expression::Literal(value) => Ok(match value {
            Literal::Number(number) => JsValue::Number(*number),
            Literal::String(text) => JsValue::String(text.clone()),
            Literal::Bool(value) => JsValue::Bool(*value),
            Literal::Null => JsValue::Null,
            Literal::Undefined => JsValue::Undefined,
        }),
        Expression::Identifier(name) => {
            Ok(runtime.read_identifier(name).unwrap_or(JsValue::Undefined))
        }
        Expression::Unary { op, value } => {
            let evaluated = eval_expression(value, runtime)?;
            Ok(match op {
                UnaryOp::Negate => JsValue::Number(-evaluated.to_number()),
                UnaryOp::Positive => JsValue::Number(evaluated.to_number()),
                UnaryOp::Not => JsValue::Bool(!evaluated.truthy()),
            })
        }
        Expression::Binary { op, left, right } => match op {
            BinaryOp::LogicalAnd => {
                let lhs = eval_expression(left, runtime)?;
                if !lhs.truthy() {
                    return Ok(lhs);
                }
                eval_expression(right, runtime)
            }
            BinaryOp::LogicalOr => {
                let lhs = eval_expression(left, runtime)?;
                if lhs.truthy() {
                    return Ok(lhs);
                }
                eval_expression(right, runtime)
            }
            BinaryOp::NullishCoalesce => {
                let lhs = eval_expression(left, runtime)?;
                if !lhs.is_nullish() {
                    return Ok(lhs);
                }
                eval_expression(right, runtime)
            }
            _ => {
                let lhs = eval_expression(left, runtime)?;
                let rhs = eval_expression(right, runtime)?;
                Ok(eval_binary(*op, lhs, rhs))
            }
        },
        Expression::Call { callee, args } => {
            let mut values = Vec::with_capacity(args.len());
            for argument in args {
                values.push(eval_expression(argument, runtime)?);
            }

            if let Some(name) = resolve_runtime_callee(callee) {
                return runtime.call_function(&name, values);
            }

            let callee_value = eval_expression(callee, runtime)?;
            if let JsValue::NativeFunction(name) = callee_value {
                return runtime.call_function(&name, values);
            }

            Ok(JsValue::Undefined)
        }
        Expression::Member { object, property } => {
            let target = eval_expression(object, runtime)?;
            Ok(target.get_property(property).unwrap_or(JsValue::Undefined))
        }
    }
}

fn resolve_runtime_callee(expression: &Expression) -> Option<String> {
    let path = flatten_member_path(expression)?;
    if path.len() == 1 {
        return path.first().cloned();
    }

    let root = path.first()?;
    if BUILTIN_RUNTIME_NAMESPACES
        .iter()
        .any(|namespace| namespace == root)
    {
        Some(path.join("."))
    } else {
        None
    }
}

fn flatten_member_path(expression: &Expression) -> Option<Vec<String>> {
    match expression {
        Expression::Identifier(name) => Some(vec![name.clone()]),
        Expression::Member { object, property } => {
            let mut parts = flatten_member_path(object)?;
            parts.push(property.clone());
            Some(parts)
        }
        _ => None,
    }
}

fn eval_binary(op: BinaryOp, lhs: JsValue, rhs: JsValue) -> JsValue {
    match op {
        BinaryOp::Add => {
            if lhs.is_string_like() || rhs.is_string_like() {
                JsValue::String(format!("{}{}", lhs.to_js_string(), rhs.to_js_string()))
            } else {
                JsValue::Number(lhs.to_number() + rhs.to_number())
            }
        }
        BinaryOp::Subtract => JsValue::Number(lhs.to_number() - rhs.to_number()),
        BinaryOp::Multiply => JsValue::Number(lhs.to_number() * rhs.to_number()),
        BinaryOp::Divide => JsValue::Number(lhs.to_number() / rhs.to_number()),
        BinaryOp::Modulo => JsValue::Number(lhs.to_number() % rhs.to_number()),
        BinaryOp::GreaterThan => JsValue::Bool(lhs.to_number() > rhs.to_number()),
        BinaryOp::GreaterThanOrEqual => JsValue::Bool(lhs.to_number() >= rhs.to_number()),
        BinaryOp::LessThan => JsValue::Bool(lhs.to_number() < rhs.to_number()),
        BinaryOp::LessThanOrEqual => JsValue::Bool(lhs.to_number() <= rhs.to_number()),
        BinaryOp::StrictEqual => JsValue::Bool(strict_equal(&lhs, &rhs)),
        BinaryOp::StrictNotEqual => JsValue::Bool(!strict_equal(&lhs, &rhs)),
        BinaryOp::Equal => JsValue::Bool(loose_equal(&lhs, &rhs)),
        BinaryOp::NotEqual => JsValue::Bool(!loose_equal(&lhs, &rhs)),
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalesce => {
            JsValue::Undefined
        }
    }
}

fn strict_equal(lhs: &JsValue, rhs: &JsValue) -> bool {
    match (lhs, rhs) {
        (JsValue::Number(a), JsValue::Number(b)) => a == b,
        (JsValue::String(a), JsValue::String(b)) => a == b,
        (JsValue::Bool(a), JsValue::Bool(b)) => a == b,
        (JsValue::Object(a), JsValue::Object(b)) => a == b,
        (JsValue::NativeFunction(a), JsValue::NativeFunction(b)) => a == b,
        (JsValue::Null, JsValue::Null) => true,
        (JsValue::Undefined, JsValue::Undefined) => true,
        _ => false,
    }
}

fn loose_equal(lhs: &JsValue, rhs: &JsValue) -> bool {
    if strict_equal(lhs, rhs) {
        return true;
    }

    if lhs.is_nullish() && rhs.is_nullish() {
        return true;
    }

    lhs.to_number() == rhs.to_number()
}
