//! `Console.prototype.table`'s real shape — see `super::table`'s call site
//! for why this exists and why it is not shared with `rts-std`'s copy of the
//! same box.
//!
//! # Cell text
//!
//! Reaches `node:util`'s own `inspect` through [`entry::call`], the same way
//! `super::inspect_one` already does for `log`/`error` (module doc's own
//! "Reuse-check") — this needed the version that ALWAYS quotes a string
//! (`util.inspect("x")` is `"'x'"`), where `inspect_one` deliberately does
//! not (a top-level `console.log("x")` prints bare). A table cell is never
//! that top-level exception, so the plain call is the right one, not a
//! second formatter.

use rts_core::entry;

/// `None` when `data` has no keys to draw at all (a number, a string,
/// `null`…) — the caller's cue to fall back to [`super::format_line`].
pub(super) fn render(data: u64) -> Option<String> {
    let rows = rows_of(data)?;

    let mut columns: Vec<String> = Vec::new();
    for row in &rows {
        match &row.kind {
            RowKind::Primitive(_) => push_unique(&mut columns, "Values"),
            RowKind::Object(members) => {
                for (key, _) in members {
                    push_unique(&mut columns, key);
                }
            }
        }
    }

    let mut header = vec!["(index)".to_owned()];
    header.extend(columns.iter().cloned());

    let grid: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            let mut line = vec![row.label.clone()];
            for column in &columns {
                let text = match &row.kind {
                    RowKind::Primitive(value) if column == "Values" => cell(*value),
                    RowKind::Object(members) => members
                        .iter()
                        .find(|(key, _)| key == column)
                        .map(|(_, value)| cell(*value))
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                line.push(text);
            }
            line
        })
        .collect();

    Some(drawn(&header, &grid))
}

enum RowKind {
    Primitive(u64),
    Object(Vec<(String, u64)>),
}

struct Row {
    label: String,
    kind: RowKind,
}

fn rows_of(data: u64) -> Option<Vec<Row>> {
    if !is_object(data) || is_callable(data) {
        return None;
    }
    let mut rows = Vec::new();
    if entry::is_array(data) {
        let length = entry::number_of(entry::get_indexed(data, string("length")))
            .filter(|n| *n >= 0.0)
            .map_or(0, |n| n as usize);
        for index in 0..length {
            let value = entry::get_indexed(data, entry::make_number(index as f64));
            rows.push(Row { label: index.to_string(), kind: row_kind(value) });
        }
    } else {
        for key in own_keys(data) {
            let value = entry::get_indexed(data, string(&key));
            rows.push(Row { label: key, kind: row_kind(value) });
        }
    }
    Some(rows)
}

fn row_kind(value: u64) -> RowKind {
    if !is_object(value) || is_callable(value) {
        return RowKind::Primitive(value);
    }
    let members = own_keys(value).into_iter().map(|key| (key.clone(), entry::get_indexed(value, string(&key)))).collect();
    RowKind::Object(members)
}

fn is_object(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_object(context, value))
}

fn is_callable(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_callable_in(context, value))
}

fn own_keys(value: u64) -> Vec<String> {
    let array = entry::own_keys(value);
    let length = entry::number_of(entry::get_indexed(array, string("length"))).map_or(0, |n| n as usize);
    (0..length).filter_map(|i| entry::described(entry::get_indexed(array, entry::make_number(i as f64)))).collect()
}

fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// One value the way `util.inspect` prints it NESTED — a string quoted.
fn cell(value: u64) -> String {
    let inspect_fn = entry::with_runtime(|context| {
        let namespace = crate::util::namespace(context);
        entry::get_member(context, namespace, "inspect")
    });
    let absent = entry::undefined_value();
    let result = entry::call(inspect_fn, absent, value, absent, absent, absent);
    entry::text_of(result).unwrap_or_default()
}

fn push_unique(columns: &mut Vec<String>, name: &str) {
    if !columns.iter().any(|existing| existing == name) {
        columns.push(name.to_owned());
    }
}

/// Identical box-drawing to `rts-std`'s copy — see that module's doc for the
/// width/centring rule, checked against real Node the same way.
fn drawn(header: &[String], rows: &[Vec<String>]) -> String {
    let mut widths = vec![0usize; header.len()];
    for (i, cell) in header.iter().enumerate() {
        widths[i] = cell.chars().count();
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    for width in &mut widths {
        *width += 2;
    }

    let rule = |left: &str, mid: &str, right: &str| -> String {
        let segments: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
        format!("{left}{}{right}", segments.join(mid))
    };
    let line = |cells: &[String]| -> String {
        let segments: Vec<String> = cells.iter().zip(&widths).map(|(text, width)| centred(text, *width)).collect();
        format!("│{}│", segments.join("│"))
    };

    let mut out = String::new();
    out.push_str(&rule("┌", "┬", "┐\n"));
    out.push_str(&line(header));
    out.push('\n');
    out.push_str(&rule("├", "┼", "┤\n"));
    for (n, row) in rows.iter().enumerate() {
        out.push_str(&line(row));
        if n + 1 < rows.len() {
            out.push('\n');
        }
    }
    out.push('\n');
    out.push_str(&rule("└", "┴", "┘"));
    out
}

fn centred(text: &str, width: usize) -> String {
    let len = text.chars().count();
    let total_pad = width.saturating_sub(len);
    let left = total_pad / 2;
    let right = total_pad - left;
    format!("{}{text}{}", " ".repeat(left), " ".repeat(right))
}
