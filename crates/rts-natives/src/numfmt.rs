//! Formatação de número JS → string (pura, sem GC/heap).
//!
//! Movida do `collector/string_pool` do `rts-runtime` pro motor porque é uma
//! função **pura** (f64 → String) reusada por várias namespaces (string_pool,
//! json, collections/vec) — vive aqui pra que camadas que não dependem do
//! backend (rts-shared) possam usá-la sem puxar o runtime.

/// Formata um `f64` como o `Number.prototype.toString()` do JS:
/// - NaN -> "NaN"
/// - +-Infinity -> "Infinity" / "-Infinity"
/// - integer dentro de [-1e21, 1e21] -> sem ponto decimal
/// - magnitude muito alta ou baixa (< 1e-6, >= 1e21) -> notation exponencial
/// - resto -> minimal decimal Rust
pub fn format_js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let abs = value.abs();
    // JS usa exponential quando abs < 1e-6 ou >= 1e21
    if abs >= 1e21 || abs < 1e-6 {
        // Rust default f64 Display: "1e308" — JS usa "1e+308". Ajustar sinal.
        let s = format!("{value:e}");
        // Garante sinal explicito no expoente: "1e308" -> "1e+308"
        if let Some(epos) = s.find('e') {
            let (mantissa, exp) = s.split_at(epos);
            let exp_body = &exp[1..]; // skip 'e'
            if !exp_body.starts_with('-') && !exp_body.starts_with('+') {
                return format!("{}e+{}", mantissa, exp_body);
            }
        }
        return s;
    }
    if value.fract() == 0.0 && abs < 1e16 {
        return format!("{}", value as i64);
    }
    format!("{value}")
}
