//! Grid Template: `none | <track-list> | subgrid <line-name-list>?` (CSS Grid Level 2 §7.1, §9.1)

use super::*;

/// Line names associated with a grid line: `[a b c]`.
pub type LineNames = Vec<String>;

/// One line specification in a subgrid template: either explicit `<line-names>` or `repeat(auto-fill, <line-names>+)`.
#[derive(Clone, PartialEq, Debug)]
pub enum SubgridLine {
    Line(LineNames),
    AutoFill(Vec<LineNames>),
}

/// An explicit track list: entries of `(line_names, track)` followed by trailing line names.
#[derive(Clone, PartialEq, Debug)]
pub struct TrackList {
    pub entries: Vec<(LineNames, GridTrack)>,
    pub trailing: LineNames,
}

impl TrackList {
    pub fn new(entries: Vec<(LineNames, GridTrack)>, trailing: LineNames) -> Self {
        Self { entries, trailing }
    }

    pub fn from_tracks(tracks: Vec<GridTrack>) -> Self {
        let entries = tracks.into_iter().map(|t| (Vec::new(), t)).collect();
        Self {
            entries,
            trailing: Vec::new(),
        }
    }

    pub fn tracks(&self) -> impl Iterator<Item = &GridTrack> {
        self.entries.iter().map(|(_, t)| t)
    }

    pub fn tracks_vec(&self) -> Vec<GridTrack> {
        self.entries.iter().map(|(_, t)| t.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn line_names_at(&self, idx: usize) -> Option<&[String]> {
        if idx < self.entries.len() {
            Some(&self.entries[idx].0)
        } else if idx == self.entries.len() {
            Some(&self.trailing)
        } else {
            None
        }
    }
}

/// A parsed grid template for `grid-template-columns` or `grid-template-rows`.
#[derive(Clone, PartialEq, Debug)]
pub enum GridTemplate {
    None,
    Tracks(TrackList),
    Subgrid(Vec<SubgridLine>),
}

impl GridTemplate {
    pub fn parse(v: &str) -> Option<GridTemplate> {
        let v = v.trim();
        if v.is_empty() {
            return None;
        }
        if v.eq_ignore_ascii_case("none") {
            return Some(GridTemplate::None);
        }
        let tokens = tokenize_template(v)?;
        if tokens.is_empty() {
            return None;
        }
        if tokens[0].eq_ignore_ascii_case("subgrid") {
            parse_subgrid(&tokens)
        } else {
            parse_track_list(&tokens).map(GridTemplate::Tracks)
        }
    }

    pub fn tracks(&self) -> Option<&TrackList> {
        match self {
            GridTemplate::Tracks(t) => Some(t),
            _ => None,
        }
    }

    pub fn is_subgrid(&self) -> bool {
        matches!(self, GridTemplate::Subgrid(_))
    }
}

fn is_valid_custom_ident(s: &str) -> bool {
    let low = s.to_ascii_lowercase();
    if matches!(
        low.as_str(),
        "span"
            | "auto"
            | "none"
            | "subgrid"
            | "inherit"
            | "initial"
            | "unset"
            | "revert"
            | "revert-layer"
            | "default"
    ) {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap_or(' ');
    if first.is_ascii_digit() {
        return false;
    }
    if first == '-' {
        if let Some(second) = chars.next() {
            if second.is_ascii_digit() {
                return false;
            }
        } else {
            return false;
        }
    }
    s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c as u32 > 0x7F)
}

fn parse_line_names_token(tok: &str) -> Option<LineNames> {
    if !tok.starts_with('[') || !tok.ends_with(']') {
        return None;
    }
    let inner = tok[1..tok.len() - 1].trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut names = Vec::new();
    for name in inner.split_whitespace() {
        if is_valid_custom_ident(name) {
            names.push(name.to_string());
        } else {
            return None;
        }
    }
    Some(names)
}

fn tokenize_template(s: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut cur = String::new();

    for ch in s.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                cur.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return None;
                }
                cur.push(ch);
                if paren_depth == 0 && bracket_depth == 0 {
                    let trimmed = cur.trim();
                    if !trimmed.is_empty() {
                        tokens.push(trimmed.to_string());
                        cur.clear();
                    }
                }
            }
            '[' => {
                if paren_depth == 0 && bracket_depth == 0 {
                    let trimmed = cur.trim();
                    if !trimmed.is_empty() {
                        tokens.push(trimmed.to_string());
                        cur.clear();
                    }
                }
                bracket_depth += 1;
                cur.push(ch);
            }
            ']' => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    return None;
                }
                cur.push(ch);
                if paren_depth == 0 && bracket_depth == 0 {
                    let trimmed = cur.trim();
                    if !trimmed.is_empty() {
                        tokens.push(trimmed.to_string());
                        cur.clear();
                    }
                }
            }
            c if c.is_whitespace() && paren_depth == 0 && bracket_depth == 0 => {
                let trimmed = cur.trim();
                if !trimmed.is_empty() {
                    tokens.push(trimmed.to_string());
                    cur.clear();
                }
            }
            c => {
                cur.push(c);
            }
        }
    }

    if paren_depth != 0 || bracket_depth != 0 {
        return None;
    }
    let trimmed = cur.trim();
    if !trimmed.is_empty() {
        tokens.push(trimmed.to_string());
    }

    Some(tokens)
}

fn parse_subgrid(tokens: &[String]) -> Option<GridTemplate> {
    if tokens.is_empty() || !tokens[0].eq_ignore_ascii_case("subgrid") {
        return None;
    }
    if tokens.len() == 1 {
        return Some(GridTemplate::Subgrid(Vec::new()));
    }
    let mut subgrid_lines = Vec::new();
    let mut has_auto_fill = false;

    for tok in &tokens[1..] {
        let t = tok.trim();
        let low = t.to_ascii_lowercase();
        if low == "subgrid" || low == "none" {
            return None;
        }
        if t.starts_with('[') && t.ends_with(']') {
            let names = parse_line_names_token(t)?;
            subgrid_lines.push(SubgridLine::Line(names));
        } else if low.starts_with("repeat(") && low.ends_with(')') {
            let inner = t[7..t.len() - 1].trim();
            let mut parts = inner.splitn(2, ',');
            let count_str = parts.next().unwrap_or("").trim();
            let pattern_str = parts.next().unwrap_or("").trim();
            if pattern_str.is_empty() {
                return None;
            }
            let pat_tokens = tokenize_template(pattern_str)?;
            if pat_tokens.is_empty() {
                return None;
            }
            let mut pat_lines = Vec::new();
            for pat_tok in pat_tokens {
                let p = pat_tok.trim();
                if p.starts_with('[') && p.ends_with(']') {
                    let names = parse_line_names_token(p)?;
                    pat_lines.push(names);
                } else {
                    return None;
                }
            }

            if count_str.eq_ignore_ascii_case("auto-fill") {
                if has_auto_fill {
                    return None;
                }
                has_auto_fill = true;
                subgrid_lines.push(SubgridLine::AutoFill(pat_lines));
            } else if count_str.eq_ignore_ascii_case("auto-fit") {
                return None;
            } else if let Ok(n) = count_str.parse::<usize>() {
                if n == 0 {
                    return None;
                }
                for _ in 0..n {
                    for line in &pat_lines {
                        subgrid_lines.push(SubgridLine::Line(line.clone()));
                    }
                }
            } else {
                return None;
            }
        } else {
            return None;
        }
    }

    Some(GridTemplate::Subgrid(subgrid_lines))
}

fn parse_track_list(tokens: &[String]) -> Option<TrackList> {
    if tokens.is_empty() {
        return None;
    }
    let mut entries = Vec::new();
    let mut pending_names = Vec::new();
    let mut has_auto_repeat = false;

    for tok in tokens {
        let t = tok.trim();
        let low = t.to_ascii_lowercase();
        if low == "subgrid" || low == "none" {
            return None;
        }
        if t.starts_with('[') && t.ends_with(']') {
            let names = parse_line_names_token(t)?;
            pending_names.extend(names);
        } else if low.starts_with("repeat(") && low.ends_with(')') {
            let inner = t[7..t.len() - 1].trim();
            let mut parts = inner.splitn(2, ',');
            let count_str = parts.next().unwrap_or("").trim();
            let tracks_str = parts.next().unwrap_or("").trim();
            if tracks_str.is_empty() {
                return None;
            }
            let inner_tokens = tokenize_template(tracks_str)?;
            let inner_tl = parse_track_list(&inner_tokens)?;

            if count_str.eq_ignore_ascii_case("auto-fill") || count_str.eq_ignore_ascii_case("auto-fit") {
                if has_auto_repeat {
                    return None;
                }
                has_auto_repeat = true;
                let count_unit: f32 = inner_tokens
                    .iter()
                    .filter(|t| !t.trim().starts_with('['))
                    .map(|t| track_count_unit(t))
                    .sum();
                let is_fit = count_str.eq_ignore_ascii_case("auto-fit");
                let auto_track = GridTrack::AutoRepeat {
                    tracks: inner_tl,
                    fit: is_fit,
                    count_unit,
                };
                entries.push((std::mem::take(&mut pending_names), auto_track));
            } else if let Ok(n) = count_str.parse::<usize>() {
                if n == 0 {
                    return None;
                }
                for _ in 0..n {
                    for (i, (l_names, tr)) in inner_tl.entries.iter().enumerate() {
                        let line = if i == 0 {
                            let mut merged = std::mem::take(&mut pending_names);
                            merged.extend(l_names.iter().cloned());
                            merged
                        } else {
                            l_names.clone()
                        };
                        entries.push((line, tr.clone()));
                    }
                    pending_names = inner_tl.trailing.clone();
                }
            } else {
                return None;
            }
        } else if let Some(track) = GridTrack::parse_one(t) {
            entries.push((std::mem::take(&mut pending_names), track));
        } else {
            return None;
        }
    }

    if entries.is_empty() {
        return None;
    }
    let trailing = pending_names;
    Some(TrackList { entries, trailing })
}

fn track_count_unit(tok: &str) -> f32 {
    let low = tok.trim().to_ascii_lowercase();
    let px_de = |s: &str| match super::lengths::parse_dimension_pub(s.trim()) {
        Some(crate::style::Dimension::Px(p)) => p,
        _ => 0.0,
    };
    if let Some(inner) = low
        .strip_prefix("minmax(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return px_de(inner.splitn(2, ',').next().unwrap_or(""));
    }
    px_de(&low)
}

