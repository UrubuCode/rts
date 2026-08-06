//! `globSync` — over the `glob` crate (see `Cargo.toml`).
//!
//! # What is implemented, and what is named instead
//!
//! - `pattern` — the single-string form only. Node's `string[]` overload
//!   (multiple patterns unioned) is not implemented; four call slots leave
//!   no room for a second pattern anyway, and a caller wanting several
//!   patterns can call this once per pattern and concatenate, which is
//!   observably the same set.
//! - `options.cwd` — read; defaults to the process's current directory.
//! - `options.exclude` — the `string | string[]` glob-pattern form is
//!   implemented (matched with the same crate against each candidate's
//!   path relative to `cwd`). The `(path: string) => boolean` PREDICATE
//!   form is refused by name: honoring it would mean calling into JS from
//!   this native, and while that is not itself unsafe here (no runtime
//!   borrow is held across the call — see [`entry::call`]'s callers in
//!   `events.rs`), it was not worth the extra argument-shape parsing this
//!   task's time did not allow; a caller needing it should filter the
//!   returned array in `.ts` instead.
//! - `options.withFileTypes` — implemented, answers `Dirent`s in place of
//!   path strings.
//! - `options.maxDepth` — NOT implemented; every match `glob` itself finds
//!   is returned regardless of depth.

use rts_core_rwk::entry;

use super::dirent;

/// One `options.exclude` pattern set, `string` or `string[]`, read the same
/// way [`super::option_flag`] already reads a `boolean` option — off the
/// object, not assumed to be a bare value.
fn exclude_patterns(options: u64) -> Vec<glob::Pattern> {
    let absent = entry::undefined_value();
    if options == absent {
        return Vec::new();
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, "exclude"));
    if value == absent {
        return Vec::new();
    }
    let mut patterns = Vec::new();
    if let Some(text) = entry::text_of(value) {
        if let Ok(pattern) = glob::Pattern::new(&text) {
            patterns.push(pattern);
        }
        return patterns;
    }
    if entry::is_array(value) {
        let length = entry::number_of(entry::get_indexed(value, super::string("length"))).unwrap_or(0.0) as usize;
        for index in 0..length {
            let element = entry::get_indexed(value, entry::make_number(index as f64));
            if let Some(text) = entry::text_of(element)
                && let Ok(pattern) = glob::Pattern::new(&text)
            {
                patterns.push(pattern);
            }
        }
    }
    patterns
}

fn string_option(options: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    entry::text_of(value)
}

/// `fs.globSync(pattern, options?)`. `undefined` when the pattern itself
/// fails to parse; an empty array (not `undefined`) when it parses and
/// simply matches nothing — a real, positive answer, unlike the "operation
/// failed" `undefined` every other member of this module reserves for a
/// genuine failure.
pub(super) extern "C" fn glob_sync(_e: u64, _this: u64, pattern: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(pattern_text) = super::text(pattern) else {
        return entry::undefined_value();
    };
    let cwd = string_option(options, "cwd").unwrap_or_else(|| ".".to_string());
    let with_file_types = super::option_flag(options, "withFileTypes");
    let excludes = exclude_patterns(options);
    let full_pattern = std::path::Path::new(&cwd).join(&pattern_text);
    let Ok(paths) = glob::glob(&full_pattern.to_string_lossy()) else {
        return entry::undefined_value();
    };
    let cwd_path = std::path::Path::new(&cwd);
    let mut matched: Vec<(String, u32)> = Vec::new();
    for found in paths.flatten() {
        let relative = found.strip_prefix(cwd_path).unwrap_or(&found).to_string_lossy().replace('\\', "/");
        if excludes.iter().any(|pattern| pattern.matches(&relative)) {
            continue;
        }
        let bits = std::fs::symlink_metadata(&found).map(|meta| dirent::type_bits(meta.file_type())).unwrap_or(0);
        matched.push((relative, bits));
    }
    entry::with_runtime(|context| {
        let values: Vec<u64> = matched
            .into_iter()
            .map(|(name, bits)| match with_file_types {
                true => {
                    let parent = std::path::Path::new(&name).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                    let entry_name = std::path::Path::new(&name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name.clone());
                    dirent::build(context, &entry_name, &parent, bits)
                }
                false => entry::make_string(context, &name),
            })
            .collect();
        entry::make_array_in(context, values)
    })
}
