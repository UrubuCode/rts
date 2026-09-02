//! An object that does not fit one cell, and lives in several consecutive ones.
//!
//! A cell holds seven words and that is the ceiling `Region::alloc` enforces.
//! One kind of object genuinely exceeds it: a generator's parked frame, laid out
//! by `rts_cranelift::frame::resumable_form` as an ordinary aggregate — label,
//! resumed value, parameters, returns and every value that outlives a
//! suspension. Six parameters and a return is already nine words.
//!
//! Nothing about the addressing changes, which is the whole reason this is the
//! answer rather than a second heap: a region's cells are consecutive words of
//! ONE allocation, so an object that runs past a cell boundary is still
//! contiguous, and `base + reference × stride` still reaches its first word.
//! Both the allocation and the access stay O(1).
//!
//! Split out of the region's own file rather than added to it because that file
//! reached the crate's 500-line ceiling, and this is a cohesive piece: the
//! allocation that spans, and the two accessors that know an object may.

use super::{Region, STRIDE};

impl Region {
    /// Takes CONSECUTIVE cells for an object of `size` bytes and type `ty`.
    ///
    /// The answer is the reference of the FIRST cell, which is what makes this
    /// work at all: a region's cells are consecutive words of one allocation, so
    /// `size` bytes from the start of a cell are contiguous whether or not they
    /// stop at its end. Compiled code computes `base + reference × stride` and
    /// then adds a byte offset — arithmetic that never asked how big the object
    /// is — so an object spanning three cells is addressed by exactly the two
    /// instructions one cell costs.
    ///
    /// Both this and the access are O(1): the allocation is a bump of `cells`
    /// rather than a search, and a field is still one offset from one base.
    ///
    /// # Why this exists beside [`Self::alloc`], rather than replacing it
    ///
    /// A spanning object hides the cells it covers from anything that walks the
    /// region by index — the following cells have no header of their own, and
    /// the collector that will need to know is not written. `alloc` keeps
    /// refusing an oversized object so that the one caller that has a reason to
    /// span has to say so, and everything else keeps the property that one
    /// reference is one cell.
    ///
    /// The reason is a generator's frame: `rts_cranelift::frame::resumable_form`
    /// lays the parked frame out as an ordinary aggregate, and six parameters
    /// and a return already need nine words against a cell's seven. The rejected
    /// alternatives were a region of its own — not expressible, since a compiled
    /// program has ONE addressing and therefore one stride — and an out-of-line
    /// spill, which the rewrite cannot use because it reaches its fields by byte
    /// offset from the frame's address.
    ///
    /// `None` when the region has not that many cells left. The header of the
    /// first cell is written; the rest are zero, as [`Self::alloc`] leaves them.
    pub fn alloc_spanning(&mut self, size: u32, ty: u32) -> Option<u32> {
        let cells = size.div_ceil(STRIDE);
        if cells == 0 {
            return None;
        }
        let index = if cells <= self.capacity - self.next {
            let taken = self.next;
            self.next += cells;
            taken
        } else if cells == 1 {
            // UMA célula não é um run, e é por aqui que ela volta.
            //
            // `Region::free` manda um objeto de uma célula para a lista LIGADA
            // e só um de duas ou mais para `free_runs` — a razão está lá e é
            // boa (enfiar um objeto largo na lista ligada espalha-o entre
            // alocações estreitas e nenhum run se volta a formar). O efeito
            // colateral é que uma alocação spanning de uma célula, que nunca
            // olha para a lista ligada, não conseguia reaproveitar NENHUMA das
            // células que ela própria libertou.
            //
            // Um frame de gerador é exatamente esse caso: `function* g() {
            // yield 1; }` cabe numa célula. Medido antes desta linha: 300 000
            // rondas de `for (const v of g())` paravam com "heap exhausted"
            // enquanto o coletor reportava `live 2366` e `freed 521867` — meio
            // milhão de células livres, e a alocação seguinte recusada. A
            // mensagem de exaustão dizia "all of them are in use", que era
            // falso; o que faltava era um caminho até elas.
            //
            // `alloc` é o caminho, e o header que ele escreve é o mesmo:
            // `alloc_spanning` calcula `width = 1 * (INLINE_SLOTS + 1) - 1`,
            // que é `INLINE_SLOTS`, que é o que `alloc` escreve.
            return self.alloc(size, ty);
        } else {
            // Nothing left to bump into, so the free list is asked — for a RUN
            // of cells and not one cell, which is why this cannot be
            // `Region::alloc`'s LIFO pop.
            //
            // Without this a wide object could only ever be born in fresh
            // space: freeing one returns its cells to a list a spanning
            // allocation never reads, so a program that grows one repeatedly —
            // an object's overflow is exactly that — walks `next` to the
            // capacity and reports the heap exhausted with a nearly empty
            // heap. Measured: 64 154 cells freed by a collection and the very
            // next spanning allocation still refused.
            self.take_free_run(cells)?
        };
        let reference = self.compose(index)?;

        // The width is every word the object covers except its own header, so a
        // `field` on it is bounded by what it actually owns rather than by a
        // cell's fifteen.
        let width = cells * (super::INLINE_SLOTS + 1) - 1;
        let at = self.word_of(index);
        self.words[at] = super::header_word(ty, width);

        // Every cell after the first has no header of its own — a sweep must
        // not mistake one for an abandoned ordinary object. See
        // `Region::spanned_interior`'s own documentation for what reading one
        // as an object would do to a live frame.
        for offset in 1..cells {
            self.mark_spanned_interior(index + offset);
        }

        Some(reference)
    }

    /// Takes `cells` CONSECUTIVE free cells out of the free list.
    ///
    /// Linear in the region, and deliberately so: it runs only when the bump
    /// space is gone, which is the moment before reporting the heap exhausted,
    /// and the alternative — a size-class free list per run length — is a
    /// second allocator to keep honest for a case that a collection normally
    /// prevents from happening at all.
    ///
    /// The free list is singly linked through the cells themselves, so a run
    /// cannot be unlinked in place. It is rebuilt instead, from the headers,
    /// which is the same scan that found the run.
    fn take_free_run(&mut self, cells: u32) -> Option<u32> {
        // First fit over the runs a collection gave back, with the remainder
        // kept as a run of its own so a big block does not swallow the space a
        // smaller object could have used.
        let at = self.free_runs.iter().position(|&(_, have)| have >= cells)?;
        let (start, have) = self.free_runs.swap_remove(at);
        if have > cells {
            self.free_runs.push((start + cells, have - cells));
        }
        Some(start)
    }

    /// Reads a field of an object that may span several cells.
    ///
    /// `slots` is ignored and kept only so a caller that knows its layout can
    /// keep saying so: the header carries the width now, so [`Region::field`]
    /// bounds a wide object by itself and this is the same call.
    pub fn spanning_field(&self, reference: u32, slot: u32, _slots: u32) -> Option<u64> {
        self.field(reference, slot)
    }

    /// Writes a field of an object that may span several cells.
    pub fn set_spanning_field(
        &mut self,
        reference: u32,
        slot: u32,
        _slots: u32,
        value: u64,
    ) -> Option<()> {
        self.set_field(reference, slot, value)
    }

    /// Returns every cell a spanning object covers.
    ///
    /// [`Region::free`] does this by itself now — it reads the width out of the
    /// header — so this is that call, kept for a caller that has the size to
    /// hand and no reason to know the header changed.
    pub fn free_spanning(&mut self, reference: u32, _size: u32) -> bool {
        self.free(reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spanning_object_takes_the_cells_it_needs_and_the_next_one_starts_after() {
        // What makes the addressing work unchanged: the reference is the first
        // cell's, and the object after it begins past the last one it covers.
        let mut region = Region::with_capacity(8);
        let wide = region.alloc_spanning(STRIDE * 2 + 8, 5).expect("room");
        let after = region.alloc(16, 6).expect("room");
        assert_eq!(wide, 0);
        assert_eq!(after, 3, "two cells plus a word of a third were taken");
        assert_eq!(region.type_of(wide), Some(5));
    }

    #[test]
    fn a_field_past_the_seventh_of_a_spanning_object_is_its_own() {
        // The slot the cell form refuses, and the reason this pair of accessors
        // exists: it continues straight through the cells the object covers.
        let mut region = Region::with_capacity(8);
        let wide = region.alloc_spanning(STRIDE * 2, 1).expect("room");
        let after = region.alloc(16, 2).expect("room");

        region
            .set_spanning_field(wide, 9, 15, 999)
            .expect("its own field");
        assert_eq!(region.spanning_field(wide, 9, 15), Some(999));
        assert_eq!(
            region.type_of(after),
            Some(2),
            "a write inside the object must not have reached the one after it"
        );
    }

    #[test]
    fn a_slot_past_what_the_object_covers_is_refused() {
        // The bound is the WIDTH in the header and not the caller's word: two
        // cells are 31 slots after one header, so the sixteenth is genuinely
        // the object's own and the thirty-second is genuinely not.
        let mut region = Region::with_capacity(8);
        let wide = region.alloc_spanning(STRIDE * 2, 1).expect("room");
        assert_eq!(region.width_of(wide), Some(31));
        assert_eq!(region.set_spanning_field(wide, 15, 15, 1), Some(()));
        assert_eq!(region.set_spanning_field(wide, 31, 15, 1), None);
        assert_eq!(region.spanning_field(wide, 31, 15), None);
    }

    #[test]
    fn a_region_without_room_for_every_cell_refuses_all_of_them() {
        // Refused rather than handing back a first cell whose later fields fall
        // outside the region: that write would land in nothing.
        let mut region = Region::with_capacity(2);
        assert_eq!(region.alloc_spanning(STRIDE * 3, 1), None);
        assert!(
            region.alloc(16, 1).is_some(),
            "a refusal takes no cells with it"
        );
    }
}

#[cfg(test)]
mod one_cell_reuse {
    use super::*;

    /// Uma alocação spanning de UMA célula reaproveita o que ela própria
    /// libertou, mesmo com o bump space esgotado.
    ///
    /// O que isto pina não é uma preferência de arrumação: sem o ramo de uma
    /// célula em [`Region::alloc_spanning`], `Region::free` mandava um frame
    /// de uma célula para a lista LIGADA e a alocação spanning só olhava para
    /// `free_runs`, que nunca recebe objetos estreitos. Um frame de gerador é
    /// exatamente isso — `function* g() { yield 1; }` cabe numa célula — e o
    /// efeito era o programa parar com "heap exhausted" enquanto o coletor
    /// reportava meio milhão de células livres.
    ///
    /// O laço enche a região de propósito: o caminho a testar é o que corre
    /// DEPOIS de o bump space acabar, e uma região com espaço livre nunca lá
    /// chega.
    #[test]
    fn a_one_cell_span_comes_back_from_the_free_list() {
        let mut region = Region::with_capacity(8);
        let mut taken = Vec::new();
        while let Some(cell) = region.alloc_spanning(STRIDE, 1) {
            taken.push(cell);
        }
        assert!(taken.len() >= 2, "a região tinha de dar mais do que uma célula");

        // Com o bump space gasto, a alocação seguinte só pode vir do que foi
        // libertado.
        assert_eq!(region.alloc_spanning(STRIDE, 1), None, "a região devia estar cheia");
        assert!(region.free(taken[0]), "libertar a primeira célula");
        let reusada = region.alloc_spanning(STRIDE, 1);
        assert!(
            reusada.is_some(),
            "uma célula acabada de libertar tem de estar ao alcance de uma \
             alocação spanning de uma célula — era este o caminho que não existia"
        );
    }

    /// E o header que o caminho novo escreve é o mesmo do caminho antigo: um
    /// campo escrito através da célula reusada lê-se de volta, o que não
    /// aconteceria se a largura tivesse ficado a zero.
    #[test]
    fn the_reused_cell_still_has_a_usable_width() {
        let mut region = Region::with_capacity(4);
        let mut taken = Vec::new();
        while let Some(cell) = region.alloc_spanning(STRIDE, 1) {
            taken.push(cell);
        }
        region.free(taken[0]);
        let cell = region.alloc_spanning(STRIDE, 1).expect("a célula libertada volta");
        assert_eq!(region.set_spanning_field(cell, 0, 1, 0x1234), Some(()));
        assert_eq!(region.spanning_field(cell, 0, 1), Some(0x1234));
    }
}
