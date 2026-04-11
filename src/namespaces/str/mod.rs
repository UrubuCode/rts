use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_usize};
use crate::namespaces::value::RuntimeValue;

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "len",
        callee: "str.len",
        doc: "Returns the byte length of a string.",
        ts_signature: "len(s: str): u64",
    },
    NamespaceMember {
        name: "concat",
        callee: "str.concat",
        doc: "Concatenates two strings.",
        ts_signature: "concat(a: str, b: str): str",
    },
    NamespaceMember {
        name: "slice",
        callee: "str.slice",
        doc: "Returns a substring from start (inclusive) to end (exclusive). Negative indices count from end.",
        ts_signature: "slice(s: str, start: i64, end?: i64): str",
    },
    NamespaceMember {
        name: "to_upper",
        callee: "str.to_upper",
        doc: "Returns the string converted to uppercase.",
        ts_signature: "to_upper(s: str): str",
    },
    NamespaceMember {
        name: "to_lower",
        callee: "str.to_lower",
        doc: "Returns the string converted to lowercase.",
        ts_signature: "to_lower(s: str): str",
    },
    NamespaceMember {
        name: "trim",
        callee: "str.trim",
        doc: "Removes leading and trailing whitespace.",
        ts_signature: "trim(s: str): str",
    },
    NamespaceMember {
        name: "trim_start",
        callee: "str.trim_start",
        doc: "Removes leading whitespace.",
        ts_signature: "trim_start(s: str): str",
    },
    NamespaceMember {
        name: "trim_end",
        callee: "str.trim_end",
        doc: "Removes trailing whitespace.",
        ts_signature: "trim_end(s: str): str",
    },
    NamespaceMember {
        name: "replace",
        callee: "str.replace",
        doc: "Replaces the first occurrence of `from` with `to`.",
        ts_signature: "replace(s: str, from: str, to: str): str",
    },
    NamespaceMember {
        name: "replace_all",
        callee: "str.replace_all",
        doc: "Replaces all occurrences of `from` with `to`.",
        ts_signature: "replace_all(s: str, from: str, to: str): str",
    },
    NamespaceMember {
        name: "includes",
        callee: "str.includes",
        doc: "Returns true if the string contains the given substring.",
        ts_signature: "includes(s: str, needle: str): bool",
    },
    NamespaceMember {
        name: "starts_with",
        callee: "str.starts_with",
        doc: "Returns true if the string starts with the given prefix.",
        ts_signature: "starts_with(s: str, prefix: str): bool",
    },
    NamespaceMember {
        name: "ends_with",
        callee: "str.ends_with",
        doc: "Returns true if the string ends with the given suffix.",
        ts_signature: "ends_with(s: str, suffix: str): bool",
    },
    NamespaceMember {
        name: "index_of",
        callee: "str.index_of",
        doc: "Returns the byte index of the first occurrence of needle, or -1 if not found.",
        ts_signature: "index_of(s: str, needle: str): i64",
    },
    NamespaceMember {
        name: "last_index_of",
        callee: "str.last_index_of",
        doc: "Returns the byte index of the last occurrence of needle, or -1 if not found.",
        ts_signature: "last_index_of(s: str, needle: str): i64",
    },
    NamespaceMember {
        name: "char_at",
        callee: "str.char_at",
        doc: "Returns the UTF-8 character at the given char index as a str.",
        ts_signature: "char_at(s: str, index: u64): str",
    },
    NamespaceMember {
        name: "split",
        callee: "str.split",
        doc: "Splits the string by separator and returns parts joined by newline (use str.split_nth to access each part).",
        ts_signature: "split(s: str, sep: str): str",
    },
    NamespaceMember {
        name: "split_nth",
        callee: "str.split_nth",
        doc: "Returns the Nth part after splitting s by sep.",
        ts_signature: "split_nth(s: str, sep: str, n: u64): str",
    },
    NamespaceMember {
        name: "repeat",
        callee: "str.repeat",
        doc: "Returns the string repeated n times.",
        ts_signature: "repeat(s: str, n: u64): str",
    },
    NamespaceMember {
        name: "pad_start",
        callee: "str.pad_start",
        doc: "Pads the string at the start to reach target length.",
        ts_signature: "pad_start(s: str, target_len: u64, fill?: str): str",
    },
    NamespaceMember {
        name: "pad_end",
        callee: "str.pad_end",
        doc: "Pads the string at the end to reach target length.",
        ts_signature: "pad_end(s: str, target_len: u64, fill?: str): str",
    },
    NamespaceMember {
        name: "char_count",
        callee: "str.char_count",
        doc: "Returns the number of Unicode scalar values (chars) in the string.",
        ts_signature: "char_count(s: str): u64",
    },
    NamespaceMember {
        name: "is_empty",
        callee: "str.is_empty",
        doc: "Returns true if the string has zero length.",
        ts_signature: "is_empty(s: str): bool",
    },
    NamespaceMember {
        name: "from_number",
        callee: "str.from_number",
        doc: "Converts a number to its string representation.",
        ts_signature: "from_number(n: f64): str",
    },
    NamespaceMember {
        name: "parse_int",
        callee: "str.parse_int",
        doc: "Parses the string as an integer. Returns NaN (as f64) on failure.",
        ts_signature: "parse_int(s: str, radix?: u64): f64",
    },
    NamespaceMember {
        name: "parse_float",
        callee: "str.parse_float",
        doc: "Parses the string as a floating-point number. Returns NaN on failure.",
        ts_signature: "parse_float(s: str): f64",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "str",
    doc: "Raw UTF-8 string primitives. These are the machine-level building blocks for the TS String class.",
    members: MEMBERS,
    ts_prelude: &[],
};

// ── Pure string helpers (no RuntimeValue, used by typed ABI symbols in abi.rs) ──────

pub fn str_len(s: &str) -> u64 {
    s.len() as u64
}

pub fn str_concat(a: &str, b: &str) -> String {
    let mut out = String::with_capacity(a.len() + b.len());
    out.push_str(a);
    out.push_str(b);
    out
}

pub fn str_slice(s: &str, start: i64, end: Option<i64>) -> String {
    let len = s.chars().count() as i64;
    let to_index = |i: i64| -> usize {
        let clamped = if i < 0 { (len + i).max(0) } else { i.min(len) };
        // Convert char index to byte index
        s.char_indices()
            .nth(clamped as usize)
            .map(|(b, _)| b)
            .unwrap_or(s.len())
    };
    let byte_start = to_index(start);
    let byte_end = end.map(to_index).unwrap_or(s.len());
    s.get(byte_start.min(byte_end)..byte_end.max(byte_start))
        .unwrap_or("")
        .to_string()
}

pub fn str_index_of(haystack: &str, needle: &str) -> i64 {
    haystack.find(needle).map(|i| i as i64).unwrap_or(-1)
}

pub fn str_last_index_of(haystack: &str, needle: &str) -> i64 {
    haystack.rfind(needle).map(|i| i as i64).unwrap_or(-1)
}

pub fn str_char_at(s: &str, index: usize) -> String {
    s.chars()
        .nth(index)
        .map(|c| c.to_string())
        .unwrap_or_default()
}

pub fn str_split(s: &str, sep: &str) -> String {
    s.split(sep).collect::<Vec<_>>().join("\n")
}

pub fn str_split_nth(s: &str, sep: &str, n: usize) -> String {
    s.split(sep).nth(n).unwrap_or("").to_string()
}

pub fn str_pad_start(s: &str, target: usize, fill: &str) -> String {
    let char_len = s.chars().count();
    if char_len >= target {
        return s.to_string();
    }
    let fill = if fill.is_empty() { " " } else { fill };
    let needed = target - char_len;
    let pad: String = fill.chars().cycle().take(needed).collect();
    format!("{pad}{s}")
}

pub fn str_pad_end(s: &str, target: usize, fill: &str) -> String {
    let char_len = s.chars().count();
    if char_len >= target {
        return s.to_string();
    }
    let fill = if fill.is_empty() { " " } else { fill };
    let needed = target - char_len;
    let pad: String = fill.chars().cycle().take(needed).collect();
    format!("{s}{pad}")
}

pub fn str_from_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

pub fn str_parse_int(s: &str, radix: u32) -> f64 {
    let radix = if radix < 2 || radix > 36 { 10 } else { radix };
    let s = s.trim();
    let (sign, s) = if let Some(rest) = s.strip_prefix('-') {
        (-1i64, rest)
    } else if let Some(rest) = s.strip_prefix('+') {
        (1, rest)
    } else {
        (1, s)
    };
    let s = if radix == 16 {
        s.strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s)
    } else {
        s
    };
    i64::from_str_radix(s, radix)
        .map(|v| (sign * v) as f64)
        .unwrap_or(f64::NAN)
}

// ── dispatch (RuntimeValue path, used by __rts_call_dispatch) ───────────────────────

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "str.len" if !args.is_empty() => {
            let s = arg_to_string(args, 0);
            Some(DispatchOutcome::Value(RuntimeValue::Number(
                str_len(&s) as f64
            )))
        }
        "str.concat" if args.len() >= 2 => {
            let a = arg_to_string(args, 0);
            let b = arg_to_string(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::String(str_concat(
                &a, &b,
            ))))
        }
        "str.slice" if !args.is_empty() => {
            let s = arg_to_string(args, 0);
            let start = args.get(1).map(|v| v.to_number() as i64).unwrap_or(0);
            let end = args.get(2).map(|v| v.to_number() as i64);
            Some(DispatchOutcome::Value(RuntimeValue::String(str_slice(
                &s, start, end,
            ))))
        }
        "str.to_upper" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            arg_to_string(args, 0).to_uppercase(),
        ))),
        "str.to_lower" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            arg_to_string(args, 0).to_lowercase(),
        ))),
        "str.trim" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            arg_to_string(args, 0).trim().to_string(),
        ))),
        "str.trim_start" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            arg_to_string(args, 0).trim_start().to_string(),
        ))),
        "str.trim_end" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            arg_to_string(args, 0).trim_end().to_string(),
        ))),
        "str.replace" if args.len() >= 3 => {
            let s = arg_to_string(args, 0);
            let from = arg_to_string(args, 1);
            let to = arg_to_string(args, 2);
            Some(DispatchOutcome::Value(RuntimeValue::String(
                s.replacen(&from, &to, 1),
            )))
        }
        "str.replace_all" if args.len() >= 3 => {
            let s = arg_to_string(args, 0);
            let from = arg_to_string(args, 1);
            let to = arg_to_string(args, 2);
            Some(DispatchOutcome::Value(RuntimeValue::String(
                s.replace(&from, &to),
            )))
        }
        "str.includes" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let needle = arg_to_string(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(
                s.contains(needle.as_str()),
            )))
        }
        "str.starts_with" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let prefix = arg_to_string(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(
                s.starts_with(prefix.as_str()),
            )))
        }
        "str.ends_with" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let suffix = arg_to_string(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(
                s.ends_with(suffix.as_str()),
            )))
        }
        "str.index_of" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let needle = arg_to_string(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::Number(
                str_index_of(&s, &needle) as f64,
            )))
        }
        "str.last_index_of" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let needle = arg_to_string(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::Number(
                str_last_index_of(&s, &needle) as f64,
            )))
        }
        "str.char_at" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let index = arg_to_usize(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::String(str_char_at(
                &s, index,
            ))))
        }
        "str.split" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let sep = arg_to_string(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::String(str_split(
                &s, &sep,
            ))))
        }
        "str.split_nth" if args.len() >= 3 => {
            let s = arg_to_string(args, 0);
            let sep = arg_to_string(args, 1);
            let n = arg_to_usize(args, 2);
            Some(DispatchOutcome::Value(RuntimeValue::String(str_split_nth(
                &s, &sep, n,
            ))))
        }
        "str.repeat" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let n = arg_to_usize(args, 1);
            Some(DispatchOutcome::Value(RuntimeValue::String(s.repeat(n))))
        }
        "str.pad_start" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let target = arg_to_usize(args, 1);
            let fill = if args.len() > 2 {
                arg_to_string(args, 2)
            } else {
                " ".to_string()
            };
            Some(DispatchOutcome::Value(RuntimeValue::String(str_pad_start(
                &s, target, &fill,
            ))))
        }
        "str.pad_end" if args.len() >= 2 => {
            let s = arg_to_string(args, 0);
            let target = arg_to_usize(args, 1);
            let fill = if args.len() > 2 {
                arg_to_string(args, 2)
            } else {
                " ".to_string()
            };
            Some(DispatchOutcome::Value(RuntimeValue::String(str_pad_end(
                &s, target, &fill,
            ))))
        }
        "str.char_count" if !args.is_empty() => {
            let s = arg_to_string(args, 0);
            Some(DispatchOutcome::Value(RuntimeValue::Number(
                s.chars().count() as f64,
            )))
        }
        "str.is_empty" if !args.is_empty() => {
            let s = arg_to_string(args, 0);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(s.is_empty())))
        }
        "str.from_number" if !args.is_empty() => {
            let n = args[0].to_number();
            Some(DispatchOutcome::Value(RuntimeValue::String(
                str_from_number(n),
            )))
        }
        "str.parse_int" if !args.is_empty() => {
            let s = arg_to_string(args, 0);
            let radix = if args.len() > 1 {
                args[1].to_number() as u32
            } else {
                10
            };
            Some(DispatchOutcome::Value(RuntimeValue::Number(str_parse_int(
                &s, radix,
            ))))
        }
        "str.parse_float" if !args.is_empty() => {
            let s = arg_to_string(args, 0);
            let v = s.trim().parse::<f64>().unwrap_or(f64::NAN);
            Some(DispatchOutcome::Value(RuntimeValue::Number(v)))
        }
        _ => None,
    }
}
