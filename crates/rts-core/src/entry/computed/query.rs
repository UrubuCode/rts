//! The computed operations that answer a QUESTION: `k in o` and `delete o[k]`.
//!
//! Apart from [`super::access`] for the reason that file states: neither of
//! these carries a value across, so neither has a second statement running user
//! code — and `delete` owns the removal both it and `Object.defineProperty`
//! perform, which is why [`remove_own`] lives here rather than beside `put`.

use super::super::objects::{machine_key, set_slot_value, slot_value};
use super::super::{Context, with_current};
use super::{opened, property_key};
use crate::object::Key;
use crate::value::Value;
/// `key in object`.
///
/// Answers whether the object HAS the property, which is not whether reading it
/// yields `undefined`: `({x: undefined})` has `x`, and `"x" in it` is true.
/// That is the whole reason the operator exists, so it is what this asks.
///
/// A receiver that is not an object answers `false` where the language throws a
/// `TypeError` — the same stated gap every property operation has, and for the
/// same reason: throwing needs protected regions and nothing emits those.
#[rtse::entry]
pub fn has_property(key: u64, object: u64) -> bool {
    // Before the borrow, for the reason `get_property` states: a trap is user
    // code and may call back in here — and so is the `toString` an object key
    // reaches through `ToPropertyKey`.
    let (key, trap) = opened(object, key);
    if let Some(named) = trap
        && let Some(answered) = super::super::proxy::has(object, named)
    {
        return answered;
    }
    with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            return false;
        };
        // An array element is own storage that no shape records, so the index
        // question is asked BEFORE the key is turned into text —
        // `ToPropertyKey` would turn `0` into `"0"` and lose it.
        //
        // It was not asked at all, and nothing below it can answer: `1 in
        // [1, 2, 3]` was false while `[1, 2, 3].hasOwnProperty(1)` was true.
        // Two spellings of one question disagreeing is the shape of defect this
        // file's own doc says the split between them must never produce, and
        // the answer was already written in `object_proto.rs` — this is the
        // same test, not a second one.
        if let Some(at) = super::super::array::as_index(context, Value(key))
            && let Some(elements) = context.elements_at(slot)
        {
            // Estar dentro do comprimento não basta: um BURACO ocupa índice e
            // não existe. `0 in [,1]` é falso; `0 in [undefined,1]` é verdadeiro.
            return elements
                .get(at)
                .is_some_and(|&held| !super::super::array::is_hole(context, held));
        }
        let Some(key) = property_key(context, Value(key)) else {
            return false;
        };
        // An accessor is a property the object HAS, and it is not in the
        // layout — so asking the shape alone answers false for
        // `"x" in { get x() {} }`, which is the operator getting its one job
        // wrong.
        !matches!(
            super::super::accessor::resolve(context, slot, key),
            super::super::accessor::Found::Absent
        )
    })
}

/// The key `length` has.
///
/// Interned rather than held as a constant, because the number is whatever the
/// registry issued — and the registry was seeded from what the compilation
/// resolved, so a program that reads `.length` already put it there and this
/// finds the same number. A program that never mentions it mints one here that
/// nothing else uses, which costs a key and answers nothing differently.
pub(in crate::entry) fn length_key(context: &mut Context) -> Key {
    context.well_known("length")
}

/// `delete o.x` / `delete o[k]`.
///
/// Answers whether the object now lacks the property, which is `true` for one
/// it never had — the language's answer for `delete` of anything that was not
/// there, including a non-object.
///
/// # Why this is a rebuild and not an unlink
///
/// `ShapeTree::remove` says it: the tree only grows, a node is shared by
/// everything that extends it, and unlinking one would change a layout other
/// objects are already using. So the shape is rebuilt without the key, which is
/// a **different layout with a different identity** — code compiled to load a
/// property at a fixed offset in the old one guards on a type number it will no
/// longer see.
///
/// The values move with it. Removing a property shifts every later one down a
/// slot, so they are read out against the old layout before the header changes
/// and written back against the new — reading after would read the new offsets
/// out of the old contents.
#[rtse::entry]
pub fn delete_property(object: u64, key: u64) -> bool {
    let (key, trap) = opened(object, key);
    if let Some(named) = trap
        && let Some(answered) = super::super::proxy::delete(object, named)
    {
        return answered;
    }
    with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            return true;
        };
        // UM ELEMENTO DE ARRAY, antes de resolver a chave como nome.
        //
        // Isto estava faltando e não era uma recusa: `delete a[0]` percorria o
        // caminho de shape, não achava o índice ali (um elemento não está na
        // shape — `PLAN` E6e diz por quê) e respondia `true` sem apagar coisa
        // alguma. Um `delete` que diz ter apagado e não apagou é a forma de erro
        // que este projeto persegue em toda parte.
        //
        // Apagar é escrever o marcador de ausência: `delete a[0]` deixa um
        // BURACO — `a.length` não muda, `a[0]` lê `undefined` e `0 in a` passa a
        // ser falso. É o único caminho que cria um buraco num array que já
        // existe; os outros dois (o literal e o crescimento) o criam ao nascer.
        if let Some(at) = super::super::array::as_index(context, Value(key)) {
            let vazio = super::super::array::hole_of(context);
            if let Some(elements) = context.elements_at_mut(slot) {
                // Além do fim não há o que remover, e `delete` responde `true`
                // por não existir — a mesma leitura que o caminho de shape faz.
                if at < elements.len() {
                    elements[at] = vazio;
                }
                return true;
            }
        }
        let Some(key) = property_key(context, Value(key)) else {
            return true;
        };
        let Some(machine) = machine_key(key) else {
            return true;
        };
        remove_own(context, slot, machine)
    })
}

/// One own property of one cell, removed — the shape half and the accessor half.
///
/// Split out of [`delete_property`] because `Object.defineProperty` needs the
/// same removal: turning a data property into an accessor has to take the key
/// OUT of the layout, or the object reports it twice — once from the shape and
/// once from the accessor table — and a cached read finds the slot the getter
/// was supposed to replace.
pub(in crate::entry) fn remove_own(
    context: &mut Context,
    slot: u32,
    machine: rts_cranelift::shape::Key,
) -> bool {
    // An accessor is an own property that no layout records — the pair is
    // deliberately outside the shape, so the walk below cannot see one and used
    // to answer `true` for a property it left exactly where it was.
    if context.accessor_at(slot, machine.index() as u32).is_some() {
        if super::super::integrity::refuses_key_removal(context, slot, machine) {
            return false;
        }
        super::super::integrity::clear_attributes(context, slot, machine);
        return context.remove_accessor(slot, machine.index() as u32);
    }
    {
        let Some(ty) = context.region.type_of(slot) else {
            return true;
        };
        let Some(shape) = context.shape_of(ty) else {
            return true;
        };
        if context.shapes.slot_of(shape, machine).is_none() {
            // Never had it. `delete` answers true, which is not "it was
            // removed" but "the object does not have it", and those agree here.
            return true;
        }
        // A sealed object keeps what it has. `delete` answers FALSE here rather
        // than failing silently: this is the one refusal in the family the
        // language reports through a return value instead of a throw, so it can
        // be said honestly without a handler to throw to.
        if super::super::integrity::refuses_key_removal(context, slot, machine) {
            return false;
        }
        // What the key permitted goes with the key. A later write creates the
        // property afresh, and the language gives a written property all three
        // — so a stale record would make `delete o.x; o.x = 1` answer with the
        // attributes of a property that no longer exists.
        super::super::integrity::clear_attributes(context, slot, machine);

        // Read every survivor against the OLD layout first.
        let kept: Vec<(rts_cranelift::shape::Key, u64)> = context
            .shapes
            .properties(shape)
            .into_iter()
            .filter(|(existing, _)| *existing != machine)
            .filter_map(|(existing, _)| {
                let at = context.shapes.slot_of(shape, existing)?;
                let value = slot_value(context, slot, at)?;
                Some((existing, value))
            })
            .collect();

        let shrunk = context.shapes.remove(shape, machine);
        // Against the same link, for the reason a growth is — and it matters
        // more here, because `ShapeTree::remove` rebuilds from the root, so what
        // comes back is the undiscriminated shape every object with those fields
        // shares. Without this, `delete o.x` merges an instance back in with
        // every other layout that happens to hold the same remainder.
        let link = context.prototype_at(slot);
        let ty = context.typed_as(shrunk, link).index() as u32;
        context.region.set_type(slot, ty);
        for (existing, value) in kept {
            if let Some(at) = context.shapes.slot_of(shrunk, existing) {
                set_slot_value(context, slot, at, value);
            }
        }
        true
    }
}
