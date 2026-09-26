//! The system font directory. The four families Blink's generic keywords
//! resolve to on Windows are found by FILE NAME — cheap, and those are the
//! faces every page without a `font-family` uses. Any other family costs one
//! scan of the directory's `name` tables, taken the first time such a family
//! is asked for and never again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::{Style, best_match};
use crate::face::{Face, family_name};

/// The four Blink defaults, `[regular, bold, italic, bold italic]`.
const KNOWN: [(&str, [&str; 4]); 4] = [
    ("times new roman", ["times.ttf", "timesbd.ttf", "timesi.ttf", "timesbi.ttf"]),
    ("arial", ["arial.ttf", "arialbd.ttf", "ariali.ttf", "arialbi.ttf"]),
    ("consolas", ["consola.ttf", "consolab.ttf", "consolai.ttf", "consolaz.ttf"]),
    ("segoe ui", ["segoeui.ttf", "segoeuib.ttf", "segoeuii.ttf", "segoeuiz.ttf"]),
];

pub(super) fn default_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        let root = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        let dir = root.join("Fonts");
        dir.is_dir().then_some(dir)
    } else {
        None
    }
}

/// One face the scan found, not yet loaded.
struct Entry {
    path: PathBuf,
    index: u32,
    weight: u16,
    italic: bool,
}

pub(super) struct SystemFonts {
    dir: Option<PathBuf>,
    /// path+index → the loaded face; a file is read at most once per store.
    loaded: Mutex<HashMap<(PathBuf, u32), Option<Arc<Face>>>>,
    /// lowercased family → faces, built on first need.
    index: OnceLock<HashMap<String, Vec<Entry>>>,
}

impl SystemFonts {
    pub(super) fn new(dir: Option<PathBuf>) -> SystemFonts {
        SystemFonts { dir, loaded: Mutex::new(HashMap::new()), index: OnceLock::new() }
    }

    pub(super) fn loaded_ids(&self) -> Vec<u64> {
        self.loaded.lock().unwrap().values().flatten().map(|f| f.id()).collect()
    }

    pub(super) fn find(&self, family: &str, weight: u16, style: Style) -> Option<Arc<Face>> {
        let dir = self.dir.as_ref()?;
        if let Some((_, files)) = KNOWN.iter().find(|(name, _)| *name == family) {
            let faces: Vec<_> = files.iter().filter_map(|file| self.load(&dir.join(file), 0)).collect();
            if !faces.is_empty() {
                return best_match(&faces, weight, style);
            }
        }
        let index = self.index.get_or_init(|| scan(dir));
        let entries = index.get(family)?;
        // Narrow to the best (weight, style) BEFORE loading, so one request
        // reads one file and not the whole family.
        let want_italic = style == Style::Italic;
        let pool: Vec<&Entry> = if entries.iter().any(|e| e.italic == want_italic) {
            entries.iter().filter(|e| e.italic == want_italic).collect()
        } else {
            entries.iter().collect()
        };
        let best = pool.into_iter().min_by_key(|e| super::weight_distance(weight, e.weight))?;
        self.load(&best.path, best.index)
    }

    fn load(&self, path: &Path, index: u32) -> Option<Arc<Face>> {
        let key = (path.to_path_buf(), index);
        if let Some(hit) = self.loaded.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let face = std::fs::read(path).ok().and_then(|bytes| Face::parse(Arc::new(bytes), index, None)).map(Arc::new);
        self.loaded.lock().unwrap().insert(key, face.clone());
        face
    }
}

/// Reads every `.ttf`/`.otf`/`.ttc` of `dir` once and keeps only the family,
/// weight and style of each face; the bytes are dropped.
fn scan(dir: &Path) -> HashMap<String, Vec<Entry>> {
    let mut out: HashMap<String, Vec<Entry>> = HashMap::new();
    let Ok(read) = std::fs::read_dir(dir) else { return out };
    for entry in read.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc" | "otc")) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let n = rustybuzz::ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
        for index in 0..n {
            let Ok(f) = rustybuzz::ttf_parser::Face::parse(&bytes, index) else { continue };
            let Some(family) = family_name(&f) else { continue };
            let weight = f.tables().os2.map_or(400, |o| o.weight().to_number());
            let italic = f.is_italic() || f.is_oblique();
            out.entry(family.to_ascii_lowercase()).or_default().push(Entry { path: path.clone(), index, weight, italic });
        }
    }
    out
}
