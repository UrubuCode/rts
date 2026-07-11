//! node:punycode — the RFC 3492 Bootstring/Punycode core (`encode`/`decode`),
//! hand-rolled per the fixed parameter set. Operates on Unicode code points
//! (`u32`); the string bridging (UTF-8 ⇄ code points) lives in the callers.
//! All internal arithmetic is overflow-checked → `Err` (a `RangeError` at the
//! JS boundary), never a panic/wrap.

const BASE: u32 = 36;
const TMIN: u32 = 1;
const TMAX: u32 = 26;
const SKEW: u32 = 38;
const DAMP: u32 = 700;
const INITIAL_BIAS: u32 = 72;
const INITIAL_N: u32 = 128;
const DELIMITER: char = '-';

/// A Bootstring failure (overflow or malformed input) → JS `RangeError`.
pub struct PunyError(pub &'static str);

fn adapt(mut delta: u32, num_points: u32, first_time: bool) -> u32 {
    delta = if first_time { delta / DAMP } else { delta / 2 };
    delta += delta / num_points;
    let mut k = 0;
    while delta > ((BASE - TMIN) * TMAX) / 2 {
        delta /= BASE - TMIN;
        k += BASE;
    }
    k + (BASE - TMIN + 1) * delta / (delta + SKEW)
}

/// Digit (0..36) → its Punycode character (`a`-`z` = 0-25, `0`-`9` = 26-35).
fn digit_to_char(d: u32) -> char {
    if d < 26 {
        (b'a' + d as u8) as char
    } else {
        (b'0' + (d - 26) as u8) as char
    }
}

/// Punycode character → digit (case-insensitive), or `None` if not a digit.
fn char_to_digit(c: char) -> Option<u32> {
    match c {
        'a'..='z' => Some(c as u32 - 'a' as u32),
        'A'..='Z' => Some(c as u32 - 'A' as u32),
        '0'..='9' => Some(c as u32 - '0' as u32 + 26),
        _ => None,
    }
}

/// RFC 3492 encode: code points → an ASCII-only Punycode string (single label,
/// no `xn--` prefix).
pub fn encode(input: &[u32]) -> Result<String, PunyError> {
    let mut output = String::new();
    let mut n = INITIAL_N;
    let mut delta: u32 = 0;
    let mut bias = INITIAL_BIAS;

    let basic_count = input.iter().filter(|&&c| c < 0x80).count() as u32;
    for &c in input.iter().filter(|&&c| c < 0x80) {
        output.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
    }
    let mut handled = basic_count;
    if basic_count > 0 {
        output.push(DELIMITER);
    }

    while (handled as usize) < input.len() {
        let m = input
            .iter()
            .copied()
            .filter(|&c| c >= n)
            .min()
            .ok_or(PunyError("Overflow: input needs wider integers to process"))?;
        delta = delta
            .checked_add(
                (m - n)
                    .checked_mul(handled + 1)
                    .ok_or(PunyError("Overflow: input needs wider integers to process"))?,
            )
            .ok_or(PunyError("Overflow: input needs wider integers to process"))?;
        n = m;
        for &c in input {
            if c < n {
                delta = delta
                    .checked_add(1)
                    .ok_or(PunyError("Overflow: input needs wider integers to process"))?;
            }
            if c == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = threshold(k, bias);
                    if q < t {
                        break;
                    }
                    let digit = t + (q - t) % (BASE - t);
                    output.push(digit_to_char(digit));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                output.push(digit_to_char(q));
                bias = adapt(delta, handled + 1, handled == basic_count);
                delta = 0;
                handled += 1;
            }
        }
        delta += 1;
        n += 1;
    }
    Ok(output)
}

/// RFC 3492 decode: an ASCII Punycode string (single label) → code points.
pub fn decode(input: &str) -> Result<Vec<u32>, PunyError> {
    let chars: Vec<char> = input.chars().collect();
    let mut output: Vec<u32> = Vec::new();
    let mut n = INITIAL_N;
    let mut i: u32 = 0;
    let mut bias = INITIAL_BIAS;

    // Basic code points: everything before the LAST delimiter.
    let mut idx = 0;
    if let Some(last_delim) = chars.iter().rposition(|&c| c == DELIMITER) {
        for &c in &chars[..last_delim] {
            if (c as u32) >= 0x80 {
                return Err(PunyError("Illegal input >= 0x80 (not a basic code point)"));
            }
            output.push(c as u32);
        }
        idx = last_delim + 1;
    }

    while idx < chars.len() {
        let old_i = i;
        let mut weight: u32 = 1;
        let mut k = BASE;
        loop {
            if idx >= chars.len() {
                return Err(PunyError("Invalid input"));
            }
            let digit = char_to_digit(chars[idx]).ok_or(PunyError("Invalid input"))?;
            idx += 1;
            i = i
                .checked_add(
                    digit
                        .checked_mul(weight)
                        .ok_or(PunyError("Overflow: input needs wider integers to process"))?,
                )
                .ok_or(PunyError("Overflow: input needs wider integers to process"))?;
            let t = threshold(k, bias);
            if digit < t {
                break;
            }
            weight = weight
                .checked_mul(BASE - t)
                .ok_or(PunyError("Overflow: input needs wider integers to process"))?;
            k += BASE;
        }
        let out_len = output.len() as u32 + 1;
        bias = adapt(i - old_i, out_len, old_i == 0);
        n = n
            .checked_add(i / out_len)
            .ok_or(PunyError("Overflow: input needs wider integers to process"))?;
        i %= out_len;
        if char::from_u32(n).is_none() {
            return Err(PunyError("Invalid input"));
        }
        output.insert(i as usize, n);
        i += 1;
    }
    Ok(output)
}

fn threshold(k: u32, bias: u32) -> u32 {
    if k <= bias {
        TMIN
    } else if k >= bias + TMAX {
        TMAX
    } else {
        k - bias
    }
}
