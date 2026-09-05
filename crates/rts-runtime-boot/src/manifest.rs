//! Reading what `rts_host::object::write_manifest` wrote.
//!
//! # Why this is hand-rolled, and how the two sides stay honest
//!
//! The writer is in `rts-host`, a crate this one neither depends on nor is
//! depended on by — naming it would be backwards, since the facade is what an
//! AOT binary links against and the host is a JIT host. So there is no shared
//! type to derive against, and the format is written out in prose in
//! `rts_host::object::manifest`'s own header and implemented twice.
//!
//! What keeps the pair honest is that neither half guesses: every table is
//! length-prefixed, every read is bounds-checked, and a short or malformed file
//! answers `None` rather than a partially-filled `Manifest`. [`crate::run`]
//! then refuses to run, which is the only safe answer — a program seeded from
//! half a key table reads every property past the break as absent, with no
//! error anywhere.

/// What `rts_host::object::write_manifest` wrote, read back.
///
/// Owns its bytes decoded into owned tables rather than borrowing the file's
/// buffer: `declare_keys`/`declare_literals` want slices of owned entries, and
/// a manifest that borrowed would tie the seed tables' lifetime to a buffer
/// this function has no reason to keep alive past seeding.
pub struct Manifest {
    pub singletons: [u32; 3],
    pub kinds: [u8; 2],
    pub keys: Vec<String>,
    /// Code units, not text. `declare_literals` says why: a string literal may
    /// be a lone surrogate, which no UTF-8 spelling can carry — and an AOT
    /// binary that lost one where a JIT run kept it would answer differently
    /// about the same program.
    pub literals: Vec<Vec<u16>>,
    pub templates: Vec<Vec<u32>>,
    /// How many entries of `__rts_modules` run before the entry.
    pub modules: usize,
    /// The frame shapes, with `code` still zero: the address is entry `n` of
    /// `__rts_frames`, and only the linker knew it.
    pub frames: Vec<rts_core::entry::FrameShape>,
    /// Name, arity, has-prototype and constructs per placed function — the
    /// address is entry `n` of `__rts_functions`.
    pub functions: Vec<(String, u32, bool, bool)>,
    /// Specifier, url and whether it is the entry, per module.
    pub metas: Vec<(String, String, bool)>,
    /// `(referrer, written, resolved)` per relative specifier the loader saw.
    pub resolutions: Vec<(String, String, String)>,
    /// One page `<script>` per entry: the hash of its exact source, paired
    /// with its position in [`Self::functions`] — the same index
    /// `FUNCTION_TABLE_SYMBOL`'s relocations are ordered by, so `main` needs
    /// no second lookup to turn one into an address. Empty for a program
    /// compiled without `--html`. See `super::page_scripts` for how this is
    /// turned into `context.eval_compiler_with_receiver`.
    pub page_scripts: Vec<(u64, u32)>,
}

/// A cursor over the manifest's bytes that can only read forward, in bounds.
///
/// Every read answers `Option`, so a truncated file stops at the first read
/// past the end instead of being detected — or not — by a length check written
/// beside each field.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn u64(&mut self) -> Option<u64> {
        let slice = self.bytes.get(self.at..self.at + 8)?;
        self.at += 8;
        Some(u64::from_le_bytes(slice.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        let slice = self.bytes.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    }

    fn u16(&mut self) -> Option<u16> {
        let slice = self.bytes.get(self.at..self.at + 2)?;
        self.at += 2;
        Some(u16::from_le_bytes(slice.try_into().ok()?))
    }

    fn u8(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn skip(&mut self, many: usize) -> Option<()> {
        self.bytes.get(self.at..self.at + many)?;
        self.at += many;
        Some(())
    }

    fn text(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        let slice = self.bytes.get(self.at..self.at + len)?;
        self.at += len;
        String::from_utf8(slice.to_vec()).ok()
    }

    /// A length-prefixed table, each entry read by `one`.
    ///
    /// The count is checked against what is LEFT before anything is allocated:
    /// a corrupt `u32` would otherwise ask for a `Vec` of four billion entries
    /// and abort on the allocation rather than answering `None`.
    fn table<T>(&mut self, mut one: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let count = self.u32()? as usize;
        if count > self.bytes.len() - self.at.min(self.bytes.len()) {
            return None;
        }
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(one(self)?);
        }
        Some(out)
    }
}

/// Reads a manifest written in `rts_host::object::manifest`'s format.
///
/// `None` on anything short of a well-formed file: a truncated read is not a
/// number this program is allowed to guess about, and [`crate::run`] treats a
/// missing manifest as a reason to abort loudly rather than run with an empty
/// key table that would make every property access read as absent.
pub fn read(bytes: &[u8]) -> Option<Manifest> {
    let mut r = Reader { bytes, at: 0 };
    let singletons = [r.u32()?, r.u32()?, r.u32()?];
    let kinds = [r.u8()?, r.u8()?];
    r.skip(2)?;

    let keys = r.table(|r| r.text())?;
    let literals = r.table(|r| r.table(|r| r.u16()))?;
    let templates = r.table(|r| r.table(|r| r.u32()))?;

    let modules = r.u32()? as usize;
    let frames = r.table(frame)?;
    let functions = r.table(|r| {
        let name = r.text()?;
        let arity = r.u32()?;
        let has_prototype = r.u8()? != 0;
        let constructs = r.u8()? != 0;
        r.skip(2)?;
        Some((name, arity, has_prototype, constructs))
    })?;
    let metas = r.table(|r| {
        let specifier = r.text()?;
        let url = r.text()?;
        let main = r.u8()? != 0;
        r.skip(3)?;
        Some((specifier, url, main))
    })?;
    let resolutions = r.table(|r| Some((r.text()?, r.text()?, r.text()?)))?;
    let page_scripts = r.table(|r| Some((r.u64()?, r.u32()?)))?;

    Some(Manifest {
        singletons,
        kinds,
        keys,
        literals,
        templates,
        modules,
        frames,
        functions,
        metas,
        resolutions,
        page_scripts,
    })
}

/// One frame shape, with `code` left at zero for the table to supply.
fn frame(r: &mut Reader<'_>) -> Option<rts_core::entry::FrameShape> {
    let ty = r.u32()?;
    let size = r.u32()?;
    let slots = r.u32()?;
    let label_field = r.u32()?;
    let resumed_field = r.u32()?;
    let mode_field = r.u32()?;
    let param_fields = r.table(|r| r.u32())?;
    // Present-or-not as its own word: field zero is a real field, so a sentinel
    // index could not tell a body returning into it from one returning nothing.
    let present = r.u32()? != 0;
    let field = r.u32()?;
    Some(rts_core::entry::FrameShape {
        code: 0,
        ty,
        size,
        slots,
        label_field,
        resumed_field,
        mode_field,
        param_fields,
        return_field: present.then_some(field),
    })
}

#[cfg(test)]
mod tests {
    use super::read;

    /// A minimal, otherwise-empty manifest, built by hand in the exact order
    /// `rts_host::object::manifest::write` emits — every table before
    /// `page_scripts` present and empty, which is what a program compiled
    /// without `--html` writes.
    fn empty_manifest_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0u32.to_le_bytes()); // undefined
        out.extend_from_slice(&1u32.to_le_bytes()); // null
        out.extend_from_slice(&2u32.to_le_bytes()); // hole
        out.push(3); // symbol kind
        out.push(4); // bigint kind
        out.extend_from_slice(&[0u8; 2]); // padding
        out.extend_from_slice(&0u32.to_le_bytes()); // keys: 0
        out.extend_from_slice(&0u32.to_le_bytes()); // literals: 0
        out.extend_from_slice(&0u32.to_le_bytes()); // templates: 0
        out.extend_from_slice(&0u32.to_le_bytes()); // modules run before entry: 0
        out.extend_from_slice(&0u32.to_le_bytes()); // frames: 0
        out.extend_from_slice(&0u32.to_le_bytes()); // functions: 0
        out.extend_from_slice(&0u32.to_le_bytes()); // metas: 0
        out.extend_from_slice(&0u32.to_le_bytes()); // resolutions: 0
        out
    }

    /// The behaviour a program with no `--html` relies on: an ABSENT
    /// `page_scripts` table (the OLD format, before this batch) is refused
    /// rather than silently treated as empty — a truncated file is not a
    /// number this reader is allowed to guess about, per its own doc.
    #[test]
    fn a_manifest_with_no_page_scripts_table_is_refused() {
        assert!(read(&empty_manifest_bytes()).is_none());
    }

    /// The table this batch adds, read back exactly as written: a hash and
    /// the function-table position beside it, in the order they were pushed.
    #[test]
    fn page_scripts_round_trip_hash_and_function_index() {
        let mut bytes = empty_manifest_bytes();
        bytes.extend_from_slice(&2u32.to_le_bytes()); // page_scripts: 2 entries
        bytes.extend_from_slice(&0xdead_beef_cafe_1234u64.to_le_bytes());
        bytes.extend_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&0x0011_2233_4455_6677u64.to_le_bytes());
        bytes.extend_from_slice(&9u32.to_le_bytes());

        let manifest = read(&bytes).expect("a well-formed manifest with two page scripts");
        assert_eq!(
            manifest.page_scripts,
            vec![(0xdead_beef_cafe_1234, 7), (0x0011_2233_4455_6677, 9)]
        );
    }

    /// A `page_scripts` table cut off mid-entry is a truncated file like any
    /// other — refused rather than answering however many WHOLE entries fit,
    /// which would silently drop the last page `<script>` a build wrote.
    #[test]
    fn a_page_scripts_table_truncated_mid_entry_is_refused() {
        let mut bytes = empty_manifest_bytes();
        bytes.extend_from_slice(&2u32.to_le_bytes()); // claims 2, holds one
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        assert!(read(&bytes).is_none());
    }
}
