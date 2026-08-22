//! The heap compiled code addresses with arithmetic.
//!
//! # Why this exists beside the slab
//!
//! [`crate::heap::Slab`] holds Rust values and hands out indices, which is what
//! a runtime written in Rust wants. Compiled code wants something else, and the
//! machine says exactly what:
//!
//! > One contiguous region: the address is a base plus a scaled index. Two
//! > instructions. This is what regional placement buys.
//!
//! A `Slab<Cell>` cannot be addressed that way. Its elements are Rust enums
//! holding `Vec`s, so there is no stride and the payload of a reference names a
//! position in a `Vec` rather than a place in memory. Every property access has
//! to become a call, which is what it was, and measured at 94.8 ns against a
//! design whose answer is a compare, a branch and a load.
//!
//! So this is a region: one allocation, fixed-stride cells, each a header
//! followed by inline slots. `base + index × stride` reaches one, which is the
//! arithmetic `lower::memory::address_of` emits.
//!
//! # Why the stride is fixed, and what that costs
//!
//! Because the addressing is `base + index × stride`. A variable-size heap needs
//! a reference that is an address, and an address is what this design refuses to
//! put in a value — an index is what makes conservative scanning safe and a
//! moving collector possible.
//!
//! What it costs is an object that does not fit. That is a real case and it has
//! a known answer — an overflow indirection, which a prior measurement in this
//! repository put at 0.25 ns — and it exists now, in `entry::objects`: a
//! property past the seventh goes to a spill beside the cell, so the region
//! keeps knowing only about the seven it holds.
//!
//! It was **not** implemented for a while, and the gap was not visible the way
//! this paragraph used to claim. Refusing the write while the read answered
//! `undefined` is precisely "a silently wrong object" — the refusal is only
//! visible if something reports it, and nothing did.
//!
//! An **allocation** that does not fit is still refused rather than truncated.
//!
//! # Several regions, and what a reference becomes
//!
//! One base address cannot serve two threads, because the base is an
//! **immediate** in the compiled code — there is one number, in the
//! instructions, and a second thread allocating against it would be allocating
//! in the first thread's memory. `rts_cranelift::mem::Addressing::Sharded` is
//! the machine's answer and it was written before anything used it: the
//! reference carries which region it belongs to, and the base is *loaded* from a
//! table the host writes once.
//!
//! So a reference is composed:
//!
//! ```text
//! reference = (cell << selector_bits) | region_index
//! ```
//!
//! and this module composes and decomposes it. That placement is the whole
//! design. [`Region`] has exactly five accessors that take a cell — [`alloc`],
//! [`field`], [`set_field`], [`set_type`], [`type_of`] — and every caller in the
//! crate reaches them through `context.region`. Teaching those five the encoding
//! means no call site learns it, and `Value::from_slot`/`as_slot` keep carrying
//! whatever number the region hands out without knowing it has parts.
//!
//! The rejected alternative was decomposing at each call site, or handing the
//! runtime a `(region, cell)` pair. Both put the encoding in a dozen places, and
//! the encoding is exactly the thing the compiler and the runtime have to agree
//! about bit for bit.
//!
//! [`alloc`]: Region::alloc
//! [`field`]: Region::field
//! [`set_field`]: Region::set_field
//! [`set_type`]: Region::set_type
//! [`type_of`]: Region::type_of
//!
//! # What a region per thread does NOT give you
//!
//! It gives each thread a heap of its own. It does **not** make a value shared
//! between threads safe, and nothing here pretends otherwise:
//!
//! - There is no `Local` versus `Shared` distinction on a region, so nothing can
//!   even state that an object escaped its thread.
//! - The write barrier records a store that crosses a region and nothing reads
//!   what it records, because the collector that would is the next piece rather
//!   than this one. See the write barrier in `entry/barrier.rs`.
//! - There is no collector, so there is nothing that would have to be told.
//!
//! Publishing a value to another thread is therefore **not expressible**: a
//! reference composed for region 3 decodes to nothing in region 5, so the read
//! answers absent rather than reading the wrong memory — which is a refusal, not
//! protection. A program that arranges to share anyway is protected by nothing.

mod growth;
mod span;

use rts_cranelift::mem::{HeaderLayout, SLOT_BYTES};

pub use growth::GROWTH_CEILING;
use growth::words_for;

/// How many inline slots a cell holds.
///
/// Fifteen, so that a cell is 128 bytes: one word of header and fifteen of
/// fields, which is two cache lines on every target this runs on.
///
/// # Why it was seven, and what changed it
///
/// Seven made a cell exactly one cache line, and this doc said so — adding
/// that the reason was alignment rather than any measurement of object sizes,
/// that how many properties a typical object has "has not been measured here",
/// and that the answer "may well not be seven". It was measured on 2026-08-11
/// and it is not seven.
///
/// What made it matter is not the memory: it is that a property past the
/// inline slots is **uncacheable**. `entry::cache::cache_resolve` answers -1
/// for `slot >= INLINE_SLOTS`, because the overflow lives in a side table a
/// compiled load cannot reach, so a read of the eighth property of an object
/// resolves BY NAME on every pass, forever. A closure's environment is an
/// object like any other, so a script with more than seven captured bindings
/// put an ordinary variable past the boundary — and every read of it in every
/// loop went through the runtime.
///
/// `bench/repro/`, two files differing by one function declaration, is what
/// that looked like: 1 135 690 cache misses and 240.9 ns for `obj.a` against
/// 75 misses and 14.4 ns. At fifteen slots both files are 14.5 ns.
///
/// # What it costs, stated rather than smoothed over
///
/// A cell is twice the size, so a region is twice the memory (8 MB at the
/// host's `CELLS`), an object straddles two cache lines instead of one, and
/// allocation-heavy code moves twice the bytes. Measured, release, 2026-08-11:
///
/// | | seven | fifteen |
/// |---|---|---|
/// | `bench/objbench.ts` | 5.66 s | **6.99 s** (+23.5%) |
/// | `bench/monte_carlo_pi.ts` | 1.53 s | **1.60 s** (+4.3%) |
/// | `bench/field_access.ts` | 61.5 ms | 54.4 ms (-11.7%) |
/// | a property read, `bench/analytic.ts` | 265 ns | 14.7 ns (-94%) |
/// | a test file in the corpus | — | -8% to -12% |
///
/// The suite is 739 of 800 either way, compared per file, with an empty LOST
/// list.
///
/// # What this does NOT fix
///
/// The cliff. It moves to the **fifteenth** property — slot 14 — and an object
/// with more of them pays exactly what the eighth used to. This said "the
/// sixteenth" and was off by one: `entry::cache` computes the reachable width as
/// `width_of − 1`, because the last slot holds the overflow block's address
/// rather than a property. Corrected 2026-08-22.
///
/// The fix that removes it rather than moving it is making the overflow
/// addressable so `cache_resolve` can answer for it — the storage already exists
/// (`spill_of`), and what is missing is the machine's half. Fifteen is what a
/// one-line change buys until then, chosen with the trade above on the table.
///
/// **And a cached STORE cannot reach the overflow even when a read can**, which
/// this note did not say. `entry::cache` resolves a store with `Reaches::Cell`
/// and answers −1 for any slot past the cell, because the lowering stores at
/// `address + offset` with no indirection — answering with an overflow offset
/// would write into the receiver's own cell at another property's position. That
/// regression shipped once (`b9df2d9d`). Measured exposure, 2026-08-22:
/// `RTS_CACHE_CENSUS=1` over 400 `tests/*.test.ts` fires that reason three times
/// in two files, neither in a loop, and zero times in `bench/analytic.ts` and
/// `bench/objbench.ts`. Dormant, and worth a fourth cache word rather than a
/// second meaning for an existing one, whenever it is closed.
///
/// # E por que não trinta e um, que é o próximo passo alinhado
///
/// Medido em 2026-08-11, depois de a escolha de quinze já estar tomada, porque
/// "aumentar o número" é a primeira ideia de quem encontra este penhasco.
///
/// Trinta e um é o próximo valor que mantém `STRIDE` potência de dois (256
/// bytes), e isso não é estética: o endereço é `base + index × stride`, e uma
/// potência de dois é um deslocamento — vinte e quatro slots dariam 200 bytes e
/// uma multiplicação de verdade em todo acesso a propriedade, além de fazer uma
/// célula atravessar linha de cache.
///
/// Mesmo binário, mesma cena, 8000 objetos de uma classe cujos campos quentes
/// JÁ CABEM nos quinze — ou seja, nada transborda em nenhuma das duas
/// configurações e a única variável é o tamanho da célula:
///
/// | | quinze | trinta e um |
/// |---|---|---|
/// | ler cinco campos, ns/objeto | ~44 | 91,8 |
/// | um `computeWorld` de motor de cena, ms/frame | **0,86** | 1,12 |
///
/// Trinta por cento pior no frame. Mover o penhasco para longe **custa**, e não
/// é uma troca neutra que só gasta memória: a densidade de cache perdida cobra
/// mais do que os slots extras compram no caso comum. O que resta consertável é
/// o penhasco ser INVISÍVEL — nada avisa que uma classe passou de quinze, e o
/// efeito aparece a 57× de distância da declaração que o causou.
/// Ver UrubuCode/rts#2171.
pub const INLINE_SLOTS: u32 = 15;

/// How far apart consecutive cells are.
pub const STRIDE: u32 = HeaderLayout::BYTES + INLINE_SLOTS * SLOT_BYTES;

/// A contiguous region of fixed-stride cells.
///
/// # Why it owns a `Vec<u64>` rather than a `Vec<u8>`
///
/// Alignment. A byte vector is aligned to one byte, and every field in a cell is
/// a machine word — so the region would be handing out addresses that a load
/// cannot use. A `u64` vector is aligned to eight, which is what correctness
/// needs.
///
/// Sixty-four-byte alignment, which would put every cell at the start of a cache
/// line rather than merely inside one, is **not** arranged. It would need an
/// over-allocation and an offset, and whether it is worth that has not been
/// measured.
///
/// # How it grows without moving
///
/// It reserves the whole span it may ever use at construction and raises a
/// bound inside it. [`growth`] is that, and holds why every other way of
/// growing was refused.
pub struct Region {
    words: Vec<u64>,
    next: u32,
    /// The bound [`Region::alloc`] enforces, which is **not** how much space the
    /// region has claimed. It starts at what the host asked for and is raised by
    /// [`Region::grow`], so a program whose garbage is collectable never pays
    /// for room it does not need.
    capacity: u32,
    /// The ceiling `capacity` may be raised to: how many cells the words
    /// allocation actually covers.
    reserved: u32,
    /// Which region this is, and what goes in the low bits of its references.
    index: u32,
    /// How many low bits that takes.
    ///
    /// Zero for a lone region, which makes composition the identity — that is
    /// what keeps every reference in a single-region program bit-for-bit what it
    /// was before this encoding existed.
    selector_bits: u32,
    /// The head of the free list, threaded through freed cells themselves.
    ///
    /// A cell index, not a reference — the same reasoning as [`Region::next`]:
    /// this is bookkeeping internal to one region, so it stays in the region's
    /// own numbering rather than in the encoding the compiler reads. `None` when
    /// nothing has been freed yet, or everything freed has been handed back out.
    free_head: Option<u32>,
    /// Which cell indices are the TRAILING cells of a spanning allocation.
    ///
    /// `span::alloc_spanning`'s own documentation says a collector "will need to
    /// know" which cells these are — this crate now has one, and this is that
    /// fact. A trailing cell has no header of its own: its word 0 is zero, which
    /// is indistinguishable by content alone from a real object of type 0 (the
    /// text type is declared first, so it IS type 0). Without this, a sweep
    /// walking every index would mistake a live generator frame's own field for
    /// an abandoned ordinary cell and write a free-list link into the middle of
    /// it — corrupting a frame that is still running. [`Self::live_refs`] is the
    /// one reader.
    spanned_interior: Vec<bool>,
    /// Wide objects that have been freed, as (first cell, how many cells).
    ///
    /// A list of its own rather than the cell list, and that is the whole
    /// design: the cell list is threaded THROUGH the cells, so a run cannot be
    /// taken out of its middle without rebuilding the list — linear per
    /// allocation, measured at +176% on `alloc class instance` and +260% on
    /// `binary TextEncoder 16`. Keeping runs apart makes reuse a pop.
    ///
    /// The cost is fragmentation in one direction: cells freed from a wide
    /// object serve another wide object and never a narrow one. Bounded by how
    /// much of a program is wide objects, and paid only after the bump space is
    /// gone.
    pub(super) free_runs: Vec<(u32, u32)>,
}

/// The header value a freed cell carries.
///
/// `Region::alloc` writes a real `TypeId` into the header, and `TypeId`s are
/// minted from zero by `rts_cranelift::types::TypeRegistry` — a registry would
/// have to declare four billion aggregates before one collided with this. That
/// margin is what makes reading the header safe as the free/live discriminant:
/// no side bitmap is kept for "is this cell alive", because the header a live
/// cell already carries answers that question for free — literally, since the
/// alternative is a second table exactly the size of `next`.
const FREE_MARKER: u64 = u64::MAX;

/// Which half of the header word says how many slots the cell owns.
///
/// The type is a `u32` and always was, so the top half of every header ever
/// written was zero. The width goes there, and that is what lets a cell say how
/// big it is without a side table and without a second word — an object wider
/// than one cell then needs no separate accessor at all, because `field` reads
/// this bound the same way it read the fixed one.
const WIDTH_SHIFT: u32 = 32;

/// A header word out of its two halves.
pub(super) fn header_word(ty: u32, width: u32) -> u64 {
    (u64::from(width) << WIDTH_SHIFT) | u64::from(ty)
}

/// The value stored in a freed cell's first slot when no cell follows it in the
/// free list.
///
/// Not `0`, because `0` is cell `0` — a real cell, and the free list's first
/// entry precisely when the very first allocation is freed. `u64::MAX` is safe
/// for the same reason [`FREE_MARKER`] is: a region's capacity is a `u32`, so no
/// real cell index reaches it.
const NO_NEXT: u64 = u64::MAX;

impl Region {
    /// A region with room for `cells` objects.
    ///
    /// `cells` is where it STARTS, not where it stops: the region reserves
    /// [`GROWTH_CEILING`] times that many cells of address space and raises the
    /// bound towards it through [`Self::grow`]. The doc on [`Region`] says why
    /// the reservation is claimed up front and why no other way of growing is
    /// available.
    ///
    /// The lone region of a single-region heap: index 0, selector width 0. Its
    /// references are cell numbers, unshifted, which is why every existing
    /// caller and every existing test is unaffected by the composition above.
    pub fn with_capacity(cells: u32) -> Self {
        Region::sharded(cells, 0, 0)
    }

    /// One region of several, numbered `index` out of `1 << selector_bits`.
    ///
    /// Separate from [`Self::with_capacity`] rather than a defaulted parameter,
    /// because the single-region case is the one that must stay free: a caller
    /// that never asks for shards must not be able to accidentally pay for them.
    pub fn sharded(cells: u32, index: u32, selector_bits: u32) -> Self {
        let reserved = cells.saturating_mul(GROWTH_CEILING);
        // Claimed ZEROED in one allocation, then shortened to the starting
        // bound. `vec![0; n]` is specialised to `alloc_zeroed`, which for a
        // block this size asks the operating system for demand-zero pages and
        // **never writes them**; `truncate` lowers the length without moving or
        // freeing anything, so the capacity — and therefore `Region::base`, and
        // therefore every address compiled code has already computed — is
        // exactly what the allocation returned.
        //
        // # Why not `reserve_exact` then `resize`, which is what this was
        //
        // Because `Vec::resize` with a zero fill is a `memset`, and it was
        // running over eight megabytes of memory the operating system had just
        // handed over — which is necessarily already zero, or one process could
        // read another's. Measured by `bench/isolated/src/bin/region_start.rs`,
        // release, 2026-08-21, per construction:
        //
        // | | ns |
        // |---|---:|
        // | `reserve_exact` + `resize(start, 0)` | **1 515 547** |
        // | `vec![0; reserved]` + `truncate(start)` | **37 814** |
        // | `reserve_exact` alone, no fill | 22 356 |
        //
        // **1.5 ms of every `rts run`**, against a whole-process budget of about
        // 7 ms above the shell's spawn floor. The remaining 15 µs over the bare
        // reservation is `truncate` and the allocator's own bookkeeping.
        //
        // # What this does NOT change
        //
        // The reservation, and the reason for it: growth may not move the base,
        // because the base is an immediate in the compiled code — `growth.rs`
        // states that and rejects `realloc`, a second region and an explicit
        // `VirtualAlloc` each with a reason. All of that is unchanged. What
        // changed is only that the starting window is not written twice.
        //
        // It does not make the memory free either. An untouched reserved page
        // still has no physical page behind it, and a page the program
        // allocates into is faulted in on first touch. The saving is the
        // *redundant* write, not the memory.
        //
        // # Where the fault moved to, and the measurement that settled it
        //
        // "exactly as before" stood in that last sentence and was **wrong**, so
        // it is corrected here rather than deleted. Before this change the
        // `resize` memset TOUCHED the first eight megabytes, so those pages were
        // resident before the program started; now they are not, and the first
        // write into each is a fault the program pays instead of the startup.
        // That is a real transfer of work from one place to another, and the
        // honest question is whether the program pays more than the startup
        // saved.
        //
        // It does not. Measured 2026-08-22, same session, alternated, a loop of
        // three million `new Callee()`: the tree WITH this change runs at
        // 104.99 / 103.11 / 98.51 ns per allocation against the tree without it
        // at 112.41 / 103.57 / 106.37. Faster, or level — the faults are spread
        // one page at a time across a program that is doing other work, where
        // the memset was eight megabytes in one blocking run before anything
        // could start.
        //
        // The reason this correction exists at all: `bench/analytic.ts` reported
        // `alloc class instance` at 108 against a table saying 90.89, which read
        // as a 20% regression and had this exact mechanism ready to explain it.
        // The A/B above is what refuted it — both binaries measure ~105 today,
        // and the 90.89 is a different day. See `docs/codegen/measurements.md`.
        let mut words = vec![0u64; words_for(reserved)];
        words.truncate(words_for(cells));
        Region {
            words,
            next: 0,
            capacity: cells,
            reserved,
            index,
            selector_bits,
            free_head: None,
            spanned_interior: vec![false; cells as usize],
            free_runs: Vec::new(),
        }
    }

    /// Which region this is.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// How many low bits of a reference name the region.
    pub fn selector_bits(&self) -> u32 {
        self.selector_bits
    }

    /// Which region a reference belongs to.
    ///
    /// Answers about a reference this region did not hand out, which is the
    /// whole reason it is not [`Self::decompose`]: that one refuses a foreign
    /// reference, and the write barrier's question is precisely *is this one
    /// foreign*.
    pub fn region_of(&self, reference: u32) -> u32 {
        reference & self.selector_mask()
    }

    /// The reference naming a cell of this region.
    ///
    /// `None` when the shift would push the cell number out of a reference.
    /// Refused rather than wrapped, for the reason an oversized allocation is:
    /// a wrapped reference names a real cell, so it is a wrong answer that looks
    /// like a right one.
    fn compose(&self, cell: u32) -> Option<u32> {
        if self.selector_bits >= u32::BITS || cell > (u32::MAX >> self.selector_bits) {
            return None;
        }
        Some((cell << self.selector_bits) | self.index)
    }

    /// The low bits a reference spends naming its region.
    fn selector_mask(&self) -> u32 {
        ((1u64 << self.selector_bits) - 1) as u32
    }

    /// The cell a reference names, when it is one of this region's.
    ///
    /// `None` for a reference belonging to another region. That is what stops a
    /// value that reached the wrong thread from reading this thread's memory at
    /// the wrong offset: it decodes to nothing and every accessor answers absent.
    /// It is a refusal, not safety — see this module's opening note.
    fn decompose(&self, reference: u32) -> Option<u32> {
        if reference & self.selector_mask() != self.index {
            return None;
        }
        Some(reference >> self.selector_bits)
    }

    /// Where the region starts.
    ///
    /// What `RegionBase::Immediate` carries. Valid for as long as this `Region`
    /// is alive and not moved — which is why a host holds it for the life of a
    /// compiled program rather than handing the address out and dropping it.
    pub fn base(&self) -> u64 {
        self.words.as_ptr() as u64
    }

    /// How far apart consecutive cells are.
    pub fn stride(&self) -> u32 {
        STRIDE
    }

    /// How many cells the region has room for **right now**.
    ///
    /// Not how many it may ever have: [`Self::grow`] raises this towards
    /// [`Self::reserved`], so a caller reporting the size of the heap wants the
    /// reservation and a caller asking whether the next allocation will bump
    /// wants this.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// How many cells have been handed out.
    pub fn used(&self) -> u32 {
        self.next
    }

    /// Takes a cell for an object of `size` bytes and type `ty`.
    ///
    /// Returns the **composed reference**, which is what a value carries: the
    /// cell number shifted past the region selector, with this region's number
    /// in the low bits. For a lone region that is the cell number itself.
    ///
    /// `None` when the region is full or the object does not fit a cell —
    /// refused rather than truncated, because an object missing its last field
    /// is a wrong answer that looks like a right one.
    ///
    /// # Why the free list is asked first
    ///
    /// LIFO: the most recently freed cell is the one handed back. It is also the
    /// one most likely still in cache, and there is no other reason to prefer
    /// one free cell over another — a fixed-size cell cannot fragment, so
    /// nothing is lost by taking whichever is cheapest to reach.
    pub fn alloc(&mut self, size: u32, ty: u32) -> Option<u32> {
        if size > STRIDE {
            return None;
        }

        if let Some(index) = self.free_head {
            let at = self.word_of(index);
            let reference = self.compose(index)?;

            // The link lived in the first slot; read it before it is
            // overwritten with the new object's field.
            let next = self.words[at + 1];
            self.free_head = if next == NO_NEXT {
                None
            } else {
                Some(next as u32)
            };

            self.words[at] = header_word(ty, INLINE_SLOTS);
            // Every slot, including the one that carried the link, is zeroed:
            // a cell reused without this would hand its new owner the previous
            // occupant's last field, which is exactly the silently wrong object
            // this crate's rule 7 keeps naming as the thing to avoid.
            for slot in 0..INLINE_SLOTS as usize {
                self.words[at + 1 + slot] = 0;
            }
            return Some(reference);
        }

        if self.next >= self.capacity {
            return None;
        }
        let index = self.next;
        let reference = self.compose(index)?;
        self.next += 1;

        // The header is one word and it is the type. The collector reads it
        // without knowing what the object is, which is the whole reason it is
        // the first thing in the cell.
        let at = self.word_of(index);
        self.words[at] = header_word(ty, INLINE_SLOTS);

        // The fields are zeroed by construction for a cell that has never been
        // handed out before. A cell coming back through the free list is zeroed
        // above instead, at reuse — not here, and not on `free`, so the cost is
        // paid exactly once per occupant rather than once per lifecycle event.
        Some(reference)
    }

    /// Gives a cell back, so a later `alloc` may hand it out again.
    ///
    /// # Double free, and why it is a hard refusal rather than a debug assertion
    ///
    /// A double free here means two owners believe they hold the same cell —
    /// the next `alloc` splices it into the free list a second time, corrupting
    /// the list into a cycle, and a subsequent allocation hands out a cell that
    /// is simultaneously still considered live elsewhere. That is silent memory
    /// corruption, not a slow path, so it is checked unconditionally — the same
    /// choice `Slab::free` already makes by folding a double free into "changes
    /// nothing" — rather than compiled out of a release build the way
    /// `debug_assert!` would be. The check is one header read, already paid to
    /// find where the cell's data lives, so paying it in release costs nothing
    /// extra.
    ///
    /// Freeing a reference that was never allocated is refused the same way:
    /// the cell index is at or past `next`, which no `alloc` has ever returned.
    ///
    /// Returns whether the cell was actually freed.
    pub fn free(&mut self, reference: u32) -> bool {
        let Some(index) = self.decompose(reference) else {
            return false;
        };
        if index >= self.next {
            return false; // never allocated
        }

        let at = self.word_of(index);
        if self.words[at] == FREE_MARKER {
            return false; // already free
        }

        // EVERY cell the object covers, not only the one its reference names.
        // A wide object spans consecutive cells and the ones after the first
        // have no header, so freeing the first alone would lose them for good:
        // nothing walks a cell marked interior, and no allocation would ever
        // reach them again.
        let width = (self.words[at] >> WIDTH_SHIFT) as u32;
        let cells = (1 + width).div_ceil(INLINE_SLOTS + 1).max(1);
        if cells > 1 {
            // A wide object's cells go back as a RUN, so the next wide object
            // can have them. Threading them onto the cell list would scatter
            // them among narrow allocations and no run would ever re-form.
            for offset in 0..cells {
                let word = self.word_of(index + offset);
                self.words[word] = FREE_MARKER;
                if let Some(flag) = self.spanned_interior.get_mut((index + offset) as usize) {
                    *flag = false;
                }
            }
            self.free_runs.push((index, cells));
            return true;
        }
        for offset in 0..cells {
            let cell = index + offset;
            if cell >= self.next {
                break;
            }
            // Threaded onto the free list through its own first slot — the slot
            // costs nothing extra because the cell's fields are dead the moment
            // it is freed.
            let word = self.word_of(cell);
            let link = match self.free_head {
                Some(next) => u64::from(next),
                None => NO_NEXT,
            };
            self.words[word] = FREE_MARKER;
            self.words[word + 1] = link;
            self.free_head = Some(cell);
            if let Some(flag) = self.spanned_interior.get_mut(cell as usize) {
                *flag = false;
            }
        }
        true
    }

    /// Reads a field of a cell, for a runtime that needs to look at one.
    ///
    /// Compiled code does not use this — it computes the address and loads. This
    /// is for the runtime's own reads, and it exists so nothing else has to
    /// know how a cell is laid out.
    pub fn field(&self, reference: u32, slot: u32) -> Option<u64> {
        if slot >= self.width_of(reference)? {
            return None;
        }
        let index = self.decompose(reference)?;
        self.words
            .get(self.word_of(index) + 1 + slot as usize)
            .copied()
    }

    /// Writes a field of a cell.
    pub fn set_field(&mut self, reference: u32, slot: u32, value: u64) -> Option<()> {
        let index = self.decompose(reference)?;
        if slot >= self.width_of(reference)? || index >= self.next {
            return None;
        }
        let at = self.word_of(index) + 1 + slot as usize;
        *self.words.get_mut(at)? = value;
        Some(())
    }

    /// Records a new type for a cell.
    ///
    /// What a property addition does: the object changed what it IS, and the
    /// header is where that is written. Nothing else in the cell moves — a
    /// transition only ever appends, so the fields already there keep their
    /// offsets.
    pub fn set_type(&mut self, reference: u32, ty: u32) -> Option<()> {
        let index = self.decompose(reference)?;
        if index >= self.next {
            return None;
        }
        let at = self.word_of(index);
        let width = (*self.words.get(at)? >> WIDTH_SHIFT) as u32;
        *self.words.get_mut(at)? = header_word(ty, width);
        Some(())
    }

    /// The type a cell's header holds.
    pub fn type_of(&self, reference: u32) -> Option<u32> {
        let index = self.decompose(reference)?;
        self.words.get(self.word_of(index)).map(|word| *word as u32)
    }

    /// How many slots a cell owns — [`INLINE_SLOTS`] for an ordinary one, and
    /// more for one taken by [`Region::alloc_spanning`].
    ///
    /// Read from the header rather than a side table, which is what makes an
    /// object wider than a cell cost nothing to reach: `field` bounds itself by
    /// this, the sweep frees by it, and the collector walks by it, all out of a
    /// word they had already loaded.
    pub fn width_of(&self, reference: u32) -> Option<u32> {
        let index = self.decompose(reference)?;
        let word = *self.words.get(self.word_of(index))?;
        if word == FREE_MARKER {
            return None;
        }
        Some((word >> WIDTH_SHIFT) as u32)
    }

    /// The whole header word, which is what an inline cache remembers.
    ///
    /// The machine compares the header it loaded against the word a cache cell
    /// holds, so remembering the TYPE alone would compare a masked value with
    /// an unmasked one and never match again. Comparing the whole word also
    /// means a site that saw a fifteen-slot object refuses a forty-slot one of
    /// the same shape — a miss, which is safe, rather than a read past the end
    /// of the narrower cell.
    pub fn header_of(&self, reference: u32) -> Option<u64> {
        let index = self.decompose(reference)?;
        self.words.get(self.word_of(index)).copied()
    }

    /// Where a cell starts, as an address.
    ///
    /// # Why the runtime is allowed to compute one at all
    ///
    /// Because one caller has to hand an address to compiled code rather than a
    /// reference: a read site that remembers where it last found something
    /// remembers a place, and a reference would have to be decomposed by the
    /// generated code, which is the arithmetic this method performs. The same
    /// arithmetic `lower::memory::address_of` emits, stated once here so the two
    /// cannot disagree about a stride or a selector.
    ///
    /// **This is not a value and must never become one.** A reference is what
    /// makes conservative scanning safe and a moving collector possible, and an
    /// address defeats both — so what comes back is for a caller that has
    /// established the cell outlives the use, and `None` for a reference this
    /// region did not hand out.
    pub fn address_of(&self, reference: u32) -> Option<u64> {
        let index = self.decompose(reference)?;
        if index >= self.next {
            return None;
        }
        Some(self.base() + u64::from(index) * u64::from(STRIDE))
    }

    /// Which word a cell starts at.
    fn word_of(&self, index: u32) -> usize {
        words_for(index)
    }

    /// Records that `index` is a trailing cell of a spanning allocation, not an
    /// object of its own. Called only by [`Self::alloc_spanning`].
    pub(super) fn mark_spanned_interior(&mut self, index: u32) {
        if let Some(flag) = self.spanned_interior.get_mut(index as usize) {
            *flag = true;
        }
    }

    /// Whether `index` is the trailing part of another object's spanning
    /// allocation — see the field's own documentation.
    fn is_spanned_interior(&self, index: u32) -> bool {
        self.spanned_interior.get(index as usize).copied().unwrap_or(false)
    }

    /// Every cell that is an object's own — the composed reference a value
    /// naming it would carry.
    ///
    /// What a sweep walks. Skips a spanning allocation's trailing cells (see
    /// [`Self::is_spanned_interior`]) and anything already on the free list —
    /// both are cells this method's caller must never treat as an independent
    /// object.
    pub fn live_refs(&self) -> Vec<u32> {
        let mut refs = Vec::new();
        self.each_live(|reference| refs.push(reference));
        refs
    }

    /// Every live cell, handed over one at a time.
    ///
    /// The form a sweep wants, and the reason it exists beside
    /// [`Self::live_refs`]: a collection was building a vector of every live
    /// cell and then a second vector of every DOOMED one, which on a heap that
    /// is 96% garbage is two allocations of tens of thousands of entries per
    /// cycle, to carry numbers that are consumed immediately and in order.
    ///
    /// Measured on a loop allocating 400 000 short-lived objects: six cycles,
    /// each freeing 62 902 cells of 65 536.
    pub fn each_live(&self, mut visit: impl FnMut(u32)) {
        for index in 0..self.next {
            if self.is_spanned_interior(index) {
                continue;
            }
            if self.words[self.word_of(index)] == FREE_MARKER {
                continue;
            }
            if let Some(reference) = self.compose(index) {
                visit(reference);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_fills_whole_cache_lines_and_never_part_of_one() {
        // This test pinned `STRIDE == 64` — a cell is ONE cache line — which
        // was the reason seven slots was seven. Fifteen is what a measurement
        // replaced it with (see `INLINE_SLOTS`), so what survives is the part
        // that was actually load-bearing: a cell begins where a line begins, so
        // reading a field never pays for a line the object does not fill.
        //
        // Stated as a test because the constant and the reason live in
        // different places, and changing one without the other is how a comment
        // starts lying — which is exactly what happened to the doc this
        // replaces.
        assert_eq!(
            STRIDE % 64,
            0,
            "a cell must be a whole number of cache lines: at {STRIDE} bytes an \
             object would start mid-line and a field read would touch a line \
             belonging to the object before it"
        );
        // E a que amarra o STRIDE aos DOIS termos que o compõem: sem ela, mudar
        // `INLINE_SLOTS` e esquecer de mudar `STRIDE` passaria neste teste
        // enquanto o alinhamento por acaso continuasse múltiplo de 64.
        assert_eq!(STRIDE, HeaderLayout::BYTES + INLINE_SLOTS * SLOT_BYTES);
    }

    #[test]
    fn the_base_is_word_aligned_because_every_field_is_a_word() {
        // What a byte vector would not give, and what a load needs.
        let region = Region::with_capacity(4);
        assert_eq!(region.base() % u64::from(SLOT_BYTES), 0);
    }

    #[test]
    fn consecutive_cells_are_one_stride_apart() {
        // The arithmetic the machine emits, checked against what this hands out:
        // `base + index * stride` must reach the cell `alloc` returned.
        let mut region = Region::with_capacity(4);
        let first = region.alloc(16, 1).expect("fits");
        let second = region.alloc(16, 2).expect("fits");
        assert_eq!(second - first, 1);
        assert_eq!(region.type_of(first), Some(1));
        assert_eq!(region.type_of(second), Some(2));
    }

    #[test]
    fn an_object_too_large_for_a_cell_is_refused_rather_than_truncated() {
        // The gap this region has, made visible. An object missing its last
        // field is a wrong answer that looks like a right one.
        let mut region = Region::with_capacity(4);
        assert_eq!(region.alloc(STRIDE + 8, 1), None);
    }

    #[test]
    fn a_full_region_refuses_rather_than_overwriting() {
        let mut region = Region::with_capacity(2);
        assert!(region.alloc(16, 1).is_some());
        assert!(region.alloc(16, 1).is_some());
        assert_eq!(region.alloc(16, 1), None);
    }

    #[test]
    fn a_field_written_is_the_field_read_and_the_neighbour_is_untouched() {
        let mut region = Region::with_capacity(2);
        let a = region.alloc(64, 1).expect("fits");
        let b = region.alloc(64, 1).expect("fits");
        region.set_field(a, 0, 111).expect("slot exists");
        region.set_field(b, 0, 222).expect("slot exists");
        assert_eq!(region.field(a, 0), Some(111));
        assert_eq!(region.field(b, 0), Some(222));
        // The header of the next cell must not be what the previous one's last
        // slot wrote — which is what an off-by-one stride would produce.
        assert_eq!(region.type_of(b), Some(1));
    }

    #[test]
    fn a_slot_past_the_inline_ones_is_refused() {
        // Where the overflow indirection will go. Refusing says the gap is here
        // rather than letting a write land in the next object.
        let mut region = Region::with_capacity(2);
        let cell = region.alloc(64, 1).expect("fits");
        assert_eq!(region.set_field(cell, INLINE_SLOTS, 1), None);
    }

    #[test]
    fn a_lone_region_hands_out_the_cell_number_itself() {
        // The property that keeps single-region programs unchanged: with no
        // selector there is nothing to shift, so a reference is what it always
        // was and no existing compiled code sees a different number.
        let mut region = Region::with_capacity(4);
        assert_eq!(region.selector_bits(), 0);
        assert_eq!(region.alloc(16, 1), Some(0));
        assert_eq!(region.alloc(16, 1), Some(1));
    }

    #[test]
    fn a_freed_cell_is_handed_out_by_the_next_alloc() {
        let mut region = Region::with_capacity(2);
        let a = region.alloc(16, 1).expect("fits");
        assert!(region.free(a));
        let b = region.alloc(16, 2).expect("the freed cell is reused");
        assert_eq!(a, b, "same cell, new occupant");
        assert_eq!(region.type_of(b), Some(2));
    }

    #[test]
    fn the_free_list_hands_cells_back_in_reverse_order_of_freeing() {
        // LIFO: the most recently freed cell is the most likely still in
        // cache, and nothing else distinguishes one free cell from another.
        let mut region = Region::with_capacity(3);
        let a = region.alloc(16, 1).expect("fits");
        let b = region.alloc(16, 1).expect("fits");
        let c = region.alloc(16, 1).expect("fits");
        assert!(region.free(a));
        assert!(region.free(b));
        assert!(region.free(c));

        assert_eq!(region.alloc(16, 9), Some(c));
        assert_eq!(region.alloc(16, 9), Some(b));
        assert_eq!(region.alloc(16, 9), Some(a));
    }

    #[test]
    fn the_free_list_survives_interleaved_alloc_and_free() {
        let mut region = Region::with_capacity(4);
        let a = region.alloc(16, 1).expect("fits");
        let b = region.alloc(16, 1).expect("fits");
        assert!(region.free(a));
        let c = region.alloc(16, 2).expect("reuses a");
        assert_eq!(a, c);
        assert!(region.free(b));
        let d = region.alloc(16, 3).expect("reuses b");
        assert_eq!(b, d);
        // The region never grew past two cells, even though four allocations
        // happened, because both reuses came from the free list.
        assert_eq!(region.used(), 2);
    }

    #[test]
    fn a_filled_region_fully_freed_can_be_filled_again() {
        let mut region = Region::with_capacity(3);
        let cells: Vec<u32> = (0..3)
            .map(|i| region.alloc(16, i).expect("fits"))
            .collect();
        assert_eq!(region.alloc(16, 99), None, "full");

        for &cell in &cells {
            assert!(region.free(cell));
        }

        let refilled: Vec<u32> = (0..3)
            .map(|i| region.alloc(16, 100 + i).expect("the region is empty again"))
            .collect();
        let mut sorted = refilled.clone();
        sorted.sort_unstable();
        let mut expected = cells.clone();
        expected.sort_unstable();
        assert_eq!(sorted, expected, "the same three cells, no more, no fewer");
        assert_eq!(region.alloc(16, 200), None, "full again");
    }

    #[test]
    fn freeing_a_cell_twice_is_refused() {
        let mut region = Region::with_capacity(2);
        let a = region.alloc(16, 1).expect("fits");
        assert!(region.free(a));
        assert!(
            !region.free(a),
            "a second free must not corrupt the list into a cycle"
        );

        // The list is still sound: exactly one alloc reuses the cell, not two
        // in a row, which is what a corrupted cycle would produce.
        let b = region.alloc(16, 2).expect("fits");
        assert_eq!(a, b);
    }

    #[test]
    fn freeing_a_cell_never_allocated_is_refused() {
        let mut region = Region::with_capacity(4);
        region.alloc(16, 1).expect("fits");
        // Cell 3 was never handed out by `alloc`.
        assert!(!region.free(3));
    }

    #[test]
    fn a_reused_cell_does_not_carry_the_previous_occupants_fields() {
        let mut region = Region::with_capacity(1);
        let a = region.alloc(16, 1).expect("fits");
        region.set_field(a, 0, 0xDEAD).expect("slot exists");
        assert!(region.free(a));

        let b = region.alloc(16, 2).expect("reused");
        assert_eq!(b, a);
        assert_eq!(
            region.field(b, 0),
            Some(0),
            "a stale field would be a wrong object that looks like a right one"
        );
    }
}
