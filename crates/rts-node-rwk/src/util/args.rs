//! `util.parseArgs` — the command-line parser.
//!
//! # Why the whole parse happens in Rust between two boundaries
//!
//! The configuration is read ambiently, the parse is Rust over `String`s, and
//! the result is built inside ONE borrow. That order is forced rather than
//! tidy: `get_indexed` and `own_keys` take the runtime borrow themselves, and
//! `make_object`/`put_member` require one already held — so a parser that built
//! its result as it read would have to hold and not hold the borrow at once.
//!
//! # What a strict-mode violation answers
//!
//! `undefined`. Node throws `ERR_PARSE_ARGS_UNKNOWN_OPTION` and friends, and
//! this engine has no catchable throw — `entry::throw` ends the program, which
//! is a far worse answer to a bad flag than a value the caller can test. Named
//! in the module's refusal list, along with the error codes that are therefore
//! unobservable.

use rts_core_rwk::entry;

use super::values::{get, is_object_value, option, own_key_strings, present, string_items, string_of};

/// One entry of `config.options`.
struct Spec {
    name: String,
    /// `type: "string"`, as opposed to `"boolean"`.
    takes_value: bool,
    multiple: bool,
    short: Option<String>,
    /// The configured default, carried as the raw value it already is: a
    /// default may be a string, a boolean or an array of either, and rebuilding
    /// it would be deciding which.
    default: u64,
}

/// What one option collected.
enum Held {
    Text(String),
    Flag(bool),
}

/// One `tokens: true` entry, before it becomes an object.
struct Token {
    kind: &'static str,
    index: usize,
    name: Option<String>,
    raw_name: Option<String>,
    value: Option<String>,
    inline: Option<bool>,
}

/// `util.parseArgs([config])`.
pub(super) extern "C" fn parse_args(
    _e: u64,
    _this: u64,
    config: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let strict = match is_object_value(config) && present(option(config, "strict")) {
        true => entry::to_boolean(option(config, "strict")),
        false => true,
    };
    let allow_positionals = match is_object_value(config) && present(option(config, "allowPositionals"))
    {
        true => entry::to_boolean(option(config, "allowPositionals")),
        false => !strict,
    };
    let allow_negative = entry::to_boolean(option(config, "allowNegative"));
    let want_tokens = entry::to_boolean(option(config, "tokens"));
    let specs = read_specs(option(config, "options"));
    let arguments = read_arguments(config);

    let Some((collected, positionals, tokens)) =
        parse(&arguments, &specs, strict, allow_positionals, allow_negative)
    else {
        return entry::undefined_value();
    };

    entry::with_runtime(|context| {
        // `undefined_in` and not the ambient `undefined_value`: this is inside
        // the borrow, and the ambient form takes a SECOND one — which an
        // `extern "C"` frame cannot unwind out of, so it aborts the process
        // rather than failing a test.
        let absent = entry::undefined_in(context);
        let values = entry::make_object(context);
        for spec in &specs {
            let held: Vec<&Held> = collected
                .iter()
                .filter(|(name, _)| *name == spec.name)
                .map(|(_, held)| held)
                .collect();
            if held.is_empty() {
                if spec.default != absent {
                    entry::put_member(context, values, &spec.name, spec.default);
                }
                continue;
            }
            let value = match spec.multiple {
                true => {
                    let items = held.iter().map(|held| held_value(context, held)).collect();
                    entry::make_array_in(context, items)
                }
                // The LAST occurrence wins for a non-`multiple` option, which is
                // what Node does and what a shell user expects from repeating a
                // flag.
                false => held_value(context, held[held.len() - 1]),
            };
            entry::put_member(context, values, &spec.name, value);
        }
        // An option nobody configured can still be collected under `strict:
        // false`; it belongs in `values` too, or a non-strict parse would drop
        // exactly the arguments it was told to tolerate.
        for (name, held) in &collected {
            if !specs.iter().any(|spec| spec.name == *name) {
                let value = held_value(context, held);
                entry::put_member(context, values, name, value);
            }
        }

        let result = entry::make_object(context);
        entry::put_member(context, result, "values", values);
        let positionals: Vec<u64> = positionals
            .iter()
            .map(|text| entry::make_string(context, text))
            .collect();
        let positionals = entry::make_array_in(context, positionals);
        entry::put_member(context, result, "positionals", positionals);
        if want_tokens {
            let built: Vec<u64> = tokens.iter().map(|token| token_object(context, token)).collect();
            let built = entry::make_array_in(context, built);
            entry::put_member(context, result, "tokens", built);
        }
        result
    })
}

/// One collected value, as a value.
fn held_value(context: &mut entry::Context, held: &Held) -> u64 {
    match held {
        Held::Text(text) => entry::make_string(context, text),
        Held::Flag(held) => entry::boolean_value(*held),
    }
}

/// One token, as an object.
fn token_object(context: &mut entry::Context, token: &Token) -> u64 {
    let object = entry::make_object(context);
    let kind = entry::make_string(context, token.kind);
    entry::put_member(context, object, "kind", kind);
    let index = entry::make_number(token.index as f64);
    entry::put_member(context, object, "index", index);
    if let Some(name) = &token.name {
        let name = entry::make_string(context, name);
        entry::put_member(context, object, "name", name);
    }
    if let Some(raw) = &token.raw_name {
        let raw = entry::make_string(context, raw);
        entry::put_member(context, object, "rawName", raw);
    }
    if let Some(value) = &token.value {
        let value = entry::make_string(context, value);
        entry::put_member(context, object, "value", value);
    }
    if let Some(inline) = token.inline {
        entry::put_member(context, object, "inlineValue", entry::boolean_value(inline));
    }
    object
}

/// `config.options`, as specs.
fn read_specs(options: u64) -> Vec<Spec> {
    if !is_object_value(options) {
        return Vec::new();
    }
    own_key_strings(options)
        .into_iter()
        .map(|name| {
            let held = get(options, &name);
            Spec {
                takes_value: string_of(get(held, "type")).as_deref() == Some("string"),
                multiple: entry::to_boolean(get(held, "multiple")),
                short: string_of(get(held, "short")),
                default: get(held, "default"),
                name,
            }
        })
        .collect()
}

/// `config.args`, or `process.argv` past the executable and the script.
///
/// Reached through the specifier table rather than through a second copy of
/// `process.argv`: `node:process` owns that array, and building one here would
/// be a second answer to what this program was invoked with.
fn read_arguments(config: u64) -> Vec<String> {
    let asked = option(config, "args");
    if entry::is_array(asked) {
        return string_items(asked);
    }
    let argv = entry::with_runtime(|context| {
        let process = entry::module_at_name(context, "node:process");
        entry::get_member(context, process, "argv")
    });
    let mut all = string_items(argv);
    // `slice(2)` — the runtime's path and the script's, which are not options.
    match all.len() >= 2 {
        true => all.split_off(2),
        false => {
            all.clear();
            all
        }
    }
}

/// The parse. `None` for a strict-mode violation.
fn parse(
    arguments: &[String],
    specs: &[Spec],
    strict: bool,
    allow_positionals: bool,
    allow_negative: bool,
) -> Option<(Vec<(String, Held)>, Vec<String>, Vec<Token>)> {
    let mut collected: Vec<(String, Held)> = Vec::new();
    let mut positionals: Vec<String> = Vec::new();
    let mut tokens: Vec<Token> = Vec::new();
    let mut at = 0usize;
    let mut only_positionals = false;

    while at < arguments.len() {
        let argument = arguments[at].as_str();
        let index = at;
        at += 1;

        if only_positionals || !argument.starts_with('-') || argument == "-" {
            if !allow_positionals && strict {
                return None;
            }
            positionals.push(argument.to_owned());
            tokens.push(Token {
                kind: "positional",
                index,
                name: None,
                raw_name: None,
                value: Some(argument.to_owned()),
                inline: None,
            });
            continue;
        }

        if argument == "--" {
            only_positionals = true;
            tokens.push(Token {
                kind: "option-terminator",
                index,
                name: None,
                raw_name: None,
                value: None,
                inline: None,
            });
            continue;
        }

        if let Some(rest) = argument.strip_prefix("--") {
            let (name, inline) = match rest.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (rest, None),
            };
            let negated = allow_negative
                && inline.is_none()
                && name.strip_prefix("no-").is_some_and(|bare| {
                    specs.iter().any(|spec| spec.name == bare && !spec.takes_value)
                });
            if negated {
                let bare = name.trim_start_matches("no-").to_owned();
                collected.push((bare.clone(), Held::Flag(false)));
                tokens.push(Token {
                    kind: "option",
                    index,
                    name: Some(bare),
                    raw_name: Some(argument.to_owned()),
                    value: None,
                    inline: None,
                });
                continue;
            }
            let spec = specs.iter().find(|spec| spec.name == name);
            if spec.is_none() && strict {
                return None;
            }
            let takes_value = spec.is_some_and(|spec| spec.takes_value);
            if !takes_value {
                // A boolean given `=value` is an error under strict mode, and
                // dropping the value silently is exactly the shape this crate
                // refuses elsewhere.
                if inline.is_some() && strict {
                    return None;
                }
                collected.push((name.to_owned(), Held::Flag(true)));
                tokens.push(Token {
                    kind: "option",
                    index,
                    name: Some(name.to_owned()),
                    raw_name: Some(format!("--{name}")),
                    value: None,
                    inline: None,
                });
                continue;
            }
            let inline_used = inline.is_some();
            let value = match inline {
                Some(value) => value,
                None => match arguments.get(at) {
                    Some(next) => {
                        at += 1;
                        next.clone()
                    }
                    None => return None,
                },
            };
            collected.push((name.to_owned(), Held::Text(value.clone())));
            tokens.push(Token {
                kind: "option",
                index,
                name: Some(name.to_owned()),
                raw_name: Some(format!("--{name}")),
                value: Some(value),
                inline: Some(inline_used),
            });
            continue;
        }

        // A short group: `-abc` is `-a -b -c` while every option but a trailing
        // string-valued one is boolean, and `-avalue` attaches the remainder to
        // a string-valued `-a`.
        let group: Vec<char> = argument[1..].chars().collect();
        let mut position = 0usize;
        while position < group.len() {
            let letter = group[position];
            let spelled = letter.to_string();
            let spec = specs.iter().find(|spec| spec.short.as_deref() == Some(spelled.as_str()));
            let Some(spec) = spec else {
                if strict {
                    return None;
                }
                collected.push((spelled.clone(), Held::Flag(true)));
                position += 1;
                continue;
            };
            if !spec.takes_value {
                collected.push((spec.name.clone(), Held::Flag(true)));
                tokens.push(Token {
                    kind: "option",
                    index,
                    name: Some(spec.name.clone()),
                    raw_name: Some(format!("-{letter}")),
                    value: None,
                    inline: None,
                });
                position += 1;
                continue;
            }
            let attached: String = group[position + 1..].iter().collect();
            let (value, inline) = match attached.is_empty() {
                false => (attached, true),
                true => match arguments.get(at) {
                    Some(next) => {
                        at += 1;
                        (next.clone(), false)
                    }
                    None => return None,
                },
            };
            collected.push((spec.name.clone(), Held::Text(value.clone())));
            tokens.push(Token {
                kind: "option",
                index,
                name: Some(spec.name.clone()),
                raw_name: Some(format!("-{letter}")),
                value: Some(value),
                inline: Some(inline),
            });
            break;
        }
    }
    Some((collected, positionals, tokens))
}
