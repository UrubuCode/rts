//! `console.table(data)` — a real box-drawing table, the shape Node prints.
//!
//! # Why this exists rather than the alias it replaces
//!
//! The doc `console::mod` used to carry said a table that does not draw a
//! table is worse than none, and that stayed true — what changed is which
//! side of it applies. `console_stub_methods.test.ts` only needed `table` to
//! exist and not throw; this task's brief asks for Node's own shape, which is
//! checkable rather than a judgement call: `(index)` header, one column per
//! own key the rows share, a `Values` column for a row that is not itself an
//! object, every cell centred in its column's width. Checked directly against
//! Node (`console.table([{a:1,b:2},{a:3,b:4}])`,
//! `console.table([1,'two',true])`), not guessed.
//!
//! # What is not implemented, by name
//!
//! The optional second argument (`properties`, a column allow-list) and
//! colour. Neither is exercised by a fixture in this repository, and both are
//! additive to the shape built here rather than a divergence from it.

use rts_core::entry;

use super::inspect::{self, Poisoned};

/// `console.table(data, properties?)`. Anything that is not an object at all —
/// a number, a string, `null`, `undefined` — has no rows and no columns to
/// draw, so it falls back to [`super::print`]'s own formatting, matching
/// Node's own behaviour for `console.table(5)`.
pub fn render(data: u64) -> Result<String, Poisoned> {
    let Some(rows) = rows_of(data)? else {
        return inspect::cell(data);
    };
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

    let mut grid = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut line = vec![row.label.clone()];
        for column in &columns {
            let text = match &row.kind {
                RowKind::Primitive(value) if column == "Values" => inspect::cell(*value)?,
                RowKind::Object(members) => match members.iter().find(|(key, _)| key == column) {
                    Some((_, value)) => inspect::cell(*value)?,
                    None => String::new(),
                },
                _ => String::new(),
            };
            line.push(text);
        }
        grid.push(line);
    }

    Ok(drawn(&header, &grid))
}

enum RowKind {
    /// Not an object: the one cell it contributes goes under `Values`.
    Primitive(u64),
    /// An object: every own key it has, read once, is a candidate column.
    Object(Vec<(String, u64)>),
}

struct Row {
    label: String,
    kind: RowKind,
}

/// The `(label, value)` pairs `console.table` draws one row per — array
/// indices for an array, own keys for a plain object — or `None` when `data`
/// has no keys to be rows AT ALL (not an object).
fn rows_of(data: u64) -> Result<Option<Vec<Row>>, Poisoned> {
    if inspect::slot_of(data).is_none() || inspect::is_callable(data) {
        return Ok(None);
    }
    let mut rows = Vec::new();
    if entry::is_array(data) {
        // `make_string` and `key_number` each take their own borrow (the
        // second AMBIENTLY, through `with_current` — the same trap
        // `console::inspect`'s own `well_known` names): calling `key_number`
        // from INSIDE `with_runtime`'s closure nests two borrows of the same
        // `RefCell` and aborts the process. Sequential, not nested — the
        // first call's borrow is released before the second is taken.
        let length_text = entry::with_runtime(|context| entry::make_string(context, "length"));
        let length_key = entry::key_number(length_text);
        let read = entry::get_property(data, length_key);
        if entry::thrown() != 0 {
            return Err(Poisoned);
        }
        let length = entry::number_of(read).filter(|n| *n >= 0.0).map_or(0, |n| n as usize);
        for index in 0..length {
            let value = entry::get_indexed(data, entry::make_number(index as f64));
            if entry::thrown() != 0 {
                return Err(Poisoned);
            }
            rows.push(Row { label: index.to_string(), kind: row_kind(value)? });
        }
    } else {
        for key in inspect::own_key_texts(data)? {
            let value = inspect::property(data, &key)?;
            rows.push(Row { label: key, kind: row_kind(value)? });
        }
    }
    Ok(Some(rows))
}

fn row_kind(value: u64) -> Result<RowKind, Poisoned> {
    if inspect::slot_of(value).is_none() || inspect::is_callable(value) {
        return Ok(RowKind::Primitive(value));
    }
    let mut members = Vec::new();
    for key in inspect::own_key_texts(value)? {
        let member = inspect::property(value, &key)?;
        members.push((key, member));
    }
    Ok(RowKind::Object(members))
}

fn push_unique(columns: &mut Vec<String>, name: &str) {
    if !columns.iter().any(|existing| existing == name) {
        columns.push(name.to_owned());
    }
}

/// The box-drawing itself: every column's width is the widest cell in it
/// (header included) plus one space of margin either side, and every cell —
/// header or data — is CENTRED in that width, extra space going right on a
/// tie. Matches Node character for character on both fixtures this was
/// checked against.
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
        let segments: Vec<String> =
            cells.iter().zip(&widths).map(|(text, width)| centred(text, *width)).collect();
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
