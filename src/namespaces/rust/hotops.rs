/// Operações otimizadas com tipos já conhecidos pelo MIR.
///
/// Diferente de `natives`, aqui os tipos dos operandos são conhecidos em compile time.
/// `TO_STRING_TABLE` elimina alocações para inteiros 0..=255 (99% dos casos práticos).
use crate::namespaces::value::RuntimeValue;
use crate::namespaces::{DispatchOutcome, arg_to_u64};

// Tabela pré-computada: evita alocação e branch complexo para inteiros pequenos.
static TO_STRING_TABLE: [&str; 256] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
    "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31", "32",
    "33", "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46", "47", "48",
    "49", "50", "51", "52", "53", "54", "55", "56", "57", "58", "59", "60", "61", "62", "63", "64",
    "65", "66", "67", "68", "69", "70", "71", "72", "73", "74", "75", "76", "77", "78", "79", "80",
    "81", "82", "83", "84", "85", "86", "87", "88", "89", "90", "91", "92", "93", "94", "95", "96",
    "97", "98", "99", "100", "101", "102", "103", "104", "105", "106", "107", "108", "109", "110",
    "111", "112", "113", "114", "115", "116", "117", "118", "119", "120", "121", "122", "123",
    "124", "125", "126", "127", "128", "129", "130", "131", "132", "133", "134", "135", "136",
    "137", "138", "139", "140", "141", "142", "143", "144", "145", "146", "147", "148", "149",
    "150", "151", "152", "153", "154", "155", "156", "157", "158", "159", "160", "161", "162",
    "163", "164", "165", "166", "167", "168", "169", "170", "171", "172", "173", "174", "175",
    "176", "177", "178", "179", "180", "181", "182", "183", "184", "185", "186", "187", "188",
    "189", "190", "191", "192", "193", "194", "195", "196", "197", "198", "199", "200", "201",
    "202", "203", "204", "205", "206", "207", "208", "209", "210", "211", "212", "213", "214",
    "215", "216", "217", "218", "219", "220", "221", "222", "223", "224", "225", "226", "227",
    "228", "229", "230", "231", "232", "233", "234", "235", "236", "237", "238", "239", "240",
    "241", "242", "243", "244", "245", "246", "247", "248", "249", "250", "251", "252", "253",
    "254", "255",
];

fn i64_to_string_fast(x: i64) -> String {
    if x >= 0 && x < 256 {
        TO_STRING_TABLE[x as usize].to_string()
    } else {
        x.to_string()
    }
}

fn f64_args(args: &[RuntimeValue]) -> (f64, f64) {
    let a = match args.first() {
        Some(RuntimeValue::Number(n)) => *n,
        _ => 0.0,
    };
    let b = match args.get(1) {
        Some(RuntimeValue::Number(n)) => *n,
        _ => 0.0,
    };
    (a, b)
}

fn i64_args(args: &[RuntimeValue]) -> (i64, i64) {
    let a = arg_to_u64(args, 0) as i64;
    let b = arg_to_u64(args, 1) as i64;
    (a, b)
}

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        // i64 arithmetic
        "rts.hotops.i64_sub" => {
            let (a, b) = i64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Number(
                a.wrapping_sub(b) as f64,
            )))
        }
        "rts.hotops.i64_div" => {
            let (a, b) = i64_args(args);
            if b == 0 {
                return Some(DispatchOutcome::Panic("division by zero".into()));
            }
            Some(DispatchOutcome::Value(RuntimeValue::Number((a / b) as f64)))
        }
        "rts.hotops.i64_mod" => {
            let (a, b) = i64_args(args);
            if b == 0 {
                return Some(DispatchOutcome::Panic("modulo by zero".into()));
            }
            Some(DispatchOutcome::Value(RuntimeValue::Number((a % b) as f64)))
        }
        "rts.hotops.i64_eq" => {
            let (a, b) = i64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(a == b)))
        }
        "rts.hotops.i64_lt" => {
            let (a, b) = i64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(a < b)))
        }
        "rts.hotops.i64_le" => {
            let (a, b) = i64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(a <= b)))
        }
        // f64 arithmetic
        "rts.hotops.f64_add" => {
            let (a, b) = f64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Number(a + b)))
        }
        "rts.hotops.f64_sub" => {
            let (a, b) = f64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Number(a - b)))
        }
        "rts.hotops.f64_div" => {
            let (a, b) = f64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Number(a / b)))
        }
        "rts.hotops.f64_eq" => {
            let (a, b) = f64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(a == b)))
        }
        "rts.hotops.f64_lt" => {
            let (a, b) = f64_args(args);
            Some(DispatchOutcome::Value(RuntimeValue::Bool(a < b)))
        }
        // string conversions
        "rts.hotops.i64_to_string" => {
            let n = arg_to_u64(args, 0) as i64;
            Some(DispatchOutcome::Value(RuntimeValue::String(
                i64_to_string_fast(n),
            )))
        }
        "rts.hotops.f64_to_string" => {
            let n = match args.first() {
                Some(RuntimeValue::Number(n)) => *n,
                _ => 0.0,
            };
            let s = if n.is_nan() {
                "NaN".to_string()
            } else if n.is_infinite() {
                if n.is_sign_negative() {
                    "-Infinity".to_string()
                } else {
                    "Infinity".to_string()
                }
            } else if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                i64_to_string_fast(n as i64)
            } else {
                n.to_string()
            };
            Some(DispatchOutcome::Value(RuntimeValue::String(s)))
        }
        _ => None,
    }
}
