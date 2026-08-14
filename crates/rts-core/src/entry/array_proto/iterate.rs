//! The array methods that call user code.
//!
//! # Why every one of these is written in two stages
//!
//! `with_current` holds a `RefCell` borrow for as long as its body runs, and the
//! callback is user code whose very first act may be to call the runtime. Calling
//! it from inside the borrow re-enters the cell — which is not a wrong answer but
//! a hang, and this repository has already paid for it once in a different shard.
//!
//! So each method collects what it needs inside a borrow, **lets the borrow go**,
//! calls, and re-borrows to store. That is the shape
//! `super::super::string::pattern::replaced` was written in, for the same reason,
//! and it is why these eight are in a file of their own: the eleven in
//! [`super`] read and answer inside one borrow, and the two shapes sitting
//! together is how one gets copied for the other.
//!
//! # Why the length is a snapshot
//!
//! The elements are copied once, before the first call. A callback that pushes
//! therefore does not extend the loop, which is what the specification says —
//! the visited range is fixed when the method starts. What it does *not* match is
//! a callback that **shortens** the array: the language stops early, and this
//! visits the elements as they were. Named rather than hidden; matching it needs
//! the loop to re-read the store between calls, which reintroduces the borrow
//! this shape exists to avoid.
//!
//! # Why `this` for the callback is `undefined` rather than the array
//!
//! Because that is what the language passes when no `thisArg` is given, and the
//! `thisArg` argument itself has nowhere to go: the four slots are spent on the
//! callback and, for `reduce`, the initial value. A method that quietly passed
//! the array would make `this` inside an ordinary arrow-free callback point at
//! something the program never asked for.

use super::super::native::Native;
use super::super::objects::undefined_of;
use super::super::{functions, with_current};
use super::super::rooted::Rooted;
use super::{built, staged};
use crate::value::Value;

/// What an array's prototype holds that takes a callback.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("forEach", for_each),
    ("map", map),
    ("filter", filter),
    ("find", find),
    ("findIndex", find_index),
    ("some", some),
    ("every", every),
    ("reduce", reduce),
];

/// `a.forEach(f)` — answers `undefined`.
/// Se esta posição deve ser PULADA por um método de iteração.
///
/// `forEach`, `map`, `filter`, `some`, `every` e `reduce` não visitam posições
/// ausentes — a especificação os define sobre as chaves que EXISTEM, não sobre
/// o intervalo `0..length`. `find`/`findIndex`/`findLast` são a exceção
/// deliberada e visitam com `undefined`; por isso `sought` converte em vez de
/// saltar.
fn vazia(held: u64) -> bool {
    super::super::with_current(|context| super::super::array::is_hole(context, held))
}

extern "C" fn for_each(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(elements) = elements_of(this) else {
        return nothing();
    };
    for (index, element) in elements.iter().enumerate() {
        if vazia(*element) {
            continue;
        }
        // A callback that throws stops the walk.
        if visit(callback, this, *element, index).is_none() {
            break;
        }
    }
    nothing()
}

/// `a.map(f)` — a new array of what `f` answered.
extern "C" fn map(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(elements) = elements_of(this) else {
        return nothing();
    };
    // ROOTED, because the callback below is user code that allocates, and an
    // allocation collects. What this has produced so far would otherwise live
    // only in a `Vec` on the Rust heap, which no scan of ours reaches — measured
    // as nine of three hundred rounds answering wrong data rather than failing.
    // See `entry::rooted`.
    let mut produced = Rooted::new();
    for (index, element) in elements.iter().enumerate() {
        // `map` PRESERVA o buraco na posição correspondente em vez de o pular:
        // o resultado tem o mesmo comprimento e a mesma esparsidade, e a
        // callback não é chamada. Empilhar `undefined` aqui daria o comprimento
        // certo e a esparsidade errada.
        if vazia(*element) {
            produced.values().push(*element);
            continue;
        }
        match visit(callback, this, *element, index) {
            Some(answered) => produced.values().push(answered),
            // The array built so far is what comes back, and the compiled call
            // site re-raises: nothing here handles the throw.
            None => break,
        }
    }
    // Built after the loop, not grown during it. `built` allocates through
    // `array_new`, which takes the context — so an array grown inside the loop
    // would be one borrow taken between two calls into user code.
    built(produced.take())
}

/// `a.filter(f)` — a new array of the elements `f` kept.
extern "C" fn filter(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(elements) = elements_of(this) else {
        return nothing();
    };
    // Rooted for the same reason `map` above is.
    let mut kept = Rooted::new();
    for (index, element) in elements.iter().enumerate() {
        // `filter` DESCARTA buracos: o resultado é denso.
        if vazia(*element) {
            continue;
        }
        match visit(callback, this, *element, index) {
            Some(answered) if truthy(answered) => kept.values().push(*element),
            Some(_) => {}
            None => break,
        }
    }
    built(kept.take())
}

/// `a.find(f)` — the first element `f` accepted, or `undefined`.
extern "C" fn find(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    match sought(this, callback, false) {
        Some((element, _)) => element,
        None => nothing(),
    }
}

/// `a.findIndex(f)` — where that element was, or -1.
///
/// Shares the scan with [`find`] rather than repeating it, because the two must
/// agree about which element matched — and a callback with a side effect makes
/// "run it twice and compare" a different program.
extern "C" fn find_index(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let at = sought(this, callback, false).map_or(-1.0, |(_, index)| index as f64);
    Value::from_f64(at).bits()
}

/// `a.some(f)`.
///
/// Stops at the first acceptance, which is observable rather than an
/// optimisation: a callback with a side effect must not run for the rest of the
/// array once the answer is settled.
extern "C" fn some(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let found = sought(this, callback, true).is_some();
    Value::from_bool(found).bits()
}

/// `a.every(f)`.
///
/// True for an empty array, which is the language and the corner an
/// implementation written as "found one that failed" gets right by construction.
extern "C" fn every(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(elements) = elements_of(this) else {
        return Value::from_bool(false).bits();
    };
    for (index, element) in elements.iter().enumerate() {
        // Um buraco não é visitado, e não faz `every` responder falso: a
        // especificação define-a sobre as chaves que existem, portanto
        // `[1,,3].every(x => x > 0)` é `true`.
        if vazia(*element) {
            continue;
        }
        match visit(callback, this, *element, index) {
            Some(answered) if truthy(answered) => {}
            // A throw and a false answer both stop the scan; only the false
            // one means the predicate said no.
            _ => return Value::from_bool(false).bits(),
        }
    }
    Value::from_bool(true).bits()
}

/// `a.reduce(f, initial)`.
///
/// Without an initial value the first element is it and the loop starts at the
/// second — not `undefined`, which would make `[1,2].reduce((a,b) => a+b)`
/// answer `NaN`. The stated gap: an empty array with no initial value is a
/// `TypeError`, and this answers `undefined`, the same gap every operation here
/// has while throwing cannot find a handler.
extern "C" fn reduce(_e: u64, this: u64, callback: u64, initial: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(elements) = elements_of(this) else {
        return nothing();
    };
    let seeded = !with_current(|context| initial == undefined_of(context));
    let (mut carried, from) = if seeded {
        (initial, 0)
    } else {
        // A semente é o primeiro elemento que EXISTE, não a posição zero: um
        // buraco à cabeça não é um acumulador, e semeá-lo com ele fazia
        // `[,1,2].reduce((a,b) => a+b)` responder `NaN`.
        match elements.iter().position(|held| !vazia(*held)) {
            Some(first) => (elements[first], first + 1),
            None => return nothing(),
        }
    };
    for (index, element) in elements.iter().enumerate().skip(from) {
        if vazia(*element) {
            continue;
        }
        let array = this;
        let receiver = nothing();
        // The accumulator takes the slot `thisArg` would have, which is the
        // arity being spent where the language spends it: `reduce` is the one
        // of these whose callback genuinely needs four arguments.
        carried = functions::call(
            callback,
            receiver,
            carried,
            *element,
            Value::from_f64(index as f64).bits(),
            array,
        );
        // A callback that threw leaves `call` answering `undefined`, and the
        // next pass would fold that into the accumulator. What comes back is
        // what had been folded before the throw; the compiled site re-raises.
        if super::super::throw::in_flight() {
            break;
        }
    }
    carried
}

/// A snapshot of the receiver's elements, when it is an array.
///
/// The borrow ends here, which is the whole point — see the module
/// documentation for what calling inside one costs.
fn elements_of(this: u64) -> Option<Vec<u64>> {
    with_current(|context| staged(context, this).map(|(_, elements)| elements))
}

/// One call of a callback, outside every borrow.
///
/// The three arguments the specification passes — element, index, and the array
/// — with `undefined` as the receiver. Written once because seven methods make
/// exactly this call, and seven copies is where one of them would pass the index
/// and the element the wrong way round.
///
/// # Why the answer is an `Option`
///
/// `None` means the callback THREW. A throw leaves `call` answering `undefined`,
/// which is a value — so without this every one of those seven would keep
/// calling the callback over the remaining elements, producing effects the
/// language says never happen. The absence is in the type so that a caller has
/// to decide rather than inherit the wrong answer.
fn visit(callback: u64, array: u64, element: u64, index: usize) -> Option<u64> {
    let (receiver, at) = with_current(|context| {
        (
            undefined_of(context),
            Value::from_f64(index as f64).bits(),
        )
    });
    let answered = functions::call(callback, receiver, element, at, array, receiver);
    match super::super::throw::in_flight() {
        true => None,
        false => Some(answered),
    }
}

/// The first element a predicate accepted, and where it was.
///
/// Shared by `find`, `findIndex` and `some`, which differ only in what they
/// report about the same scan.
fn sought(this: u64, callback: u64, skip_holes: bool) -> Option<(u64, usize)> {
    let elements = elements_of(this)?;
    for (index, element) in elements.iter().enumerate() {
        // `some` é o chamador que SALTA, e por isso o parâmetro existe: ele
        // pertence à família de `forEach`/`map`/`filter`, definida sobre as
        // chaves que existem, e partilha este scan com `find`/`findIndex`, que
        // pertencem à outra. Um scan e dois contratos precisa de dizer qual.
        if skip_holes && vazia(*element) {
            continue;
        }
        // `find`/`findIndex` NÃO pulam buracos — a especificação manda visitar
        // a posição com `undefined`, ao contrário de `forEach`/`map`/`filter`.
        // Então aqui o buraco é convertido em vez de saltado, e o valor
        // devolvido é o convertido: este é o quarto ponto que entrega o word
        // direto ao programa.
        let visivel = super::super::with_current(|context| {
            super::super::array::visible(context, *element)
        });
        match visit(callback, this, visivel, index) {
            Some(answered) if truthy(answered) => return Some((visivel, index)),
            Some(_) => {}
            None => return None,
        }
    }
    None
}

/// Whether a callback's answer counts as yes.
///
/// Through the language's falsy set rather than a comparison against `true`: a
/// predicate answering `1` or `"a"` accepts, and one answering `0` or `""` does
/// not. Comparing to `true` is the version that looks right and rejects every
/// idiomatic predicate.
fn truthy(value: u64) -> bool {
    super::super::primitives::to_boolean(value)
}

/// The `undefined` a method answers when there is nothing to answer.
fn nothing() -> u64 {
    with_current(|context| undefined_of(context))
}
