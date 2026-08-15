//! The two computed operations that MOVE a value: `o[k]` and `o[k] = v`.
//!
//! Apart from [`super::query`] because the question they answer is different
//! rather than merely larger: these two carry a value across, so each has a
//! second statement — the getter or the setter — that runs with no borrow
//! held. `in` and `delete` answer a boolean and never call anything.

use super::super::objects::{put, undefined_of};
use super::super::string::text::{string_element, string_property};
use super::super::with_current;
use super::{opened, primitive_found, property_key};
use crate::value::Value;

/// `object[key]`, where the key is a value rather than a resolved name.
///
/// Two statements, like the named read and for the same reason: the answer may
/// be a getter, which is user code that must not run inside a borrow of the
/// context.
#[rtse::entry]
pub fn get_indexed(object: u64, key: u64) -> u64 {
    // The computed spelling of the read `get_property` performs, and a proxy
    // has to be asked by BOTH: `o.x` reaches one and `Reflect.get(o, "x")` the
    // other, and a trap that answered one of them and not the other would be
    // two spellings of one operation disagreeing — the failure this file exists
    // to keep out.
    // A pergunta ao proxy vem DEPOIS de saber que há um proxy, e não antes.
    //
    // Resolver a chave de um índice numérico é `ToPropertyKey`: formatar o
    // double como texto (`format!("{:e}")` mais duas alocações), depois internar
    // a string. Isso rodava em TODO acesso indexado — e o resultado era jogado
    // fora, porque o caminho de elemento logo abaixo volta a tratar o índice
    // como número (`array::as_index`).
    //
    // Um proxy continua sendo consultado exatamente como antes; o que mudou é
    // que quem não é proxy não paga mais a conversão.
    let (key, trap) = opened(object, key);
    if let Some(named) = trap
        && let Some(answered) = super::super::proxy::get(object, named)
    {
        return answered;
    }
    let found = with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            // A number, a boolean, a symbol or a bigint — none of which has a
            // cell to walk from. The same fallback [`super::super::objects::get_property`]
            // makes, and it was missing here: `(255).toString(16)` answered
            // `"ff"` while `(255)["toString"](16)` answered `undefined`.
            //
            // Two spellings of one operation cannot differ on what the receiver
            // IS. This file's own documentation says why the two are split —
            // how a KEY is resolved — and that argument stops at the key: by
            // the time either path reaches a receiver, "a key is a key".
            let Some(key) = property_key(context, Value(key)) else {
                return super::super::accessor::Found::Value(undefined_of(context));
            };
            return primitive_found(context, Value(object), key);
        };
        // An element, if this is an array and the key is a canonical index.
        // Asked BEFORE `ToPropertyKey`, because that would turn the number into
        // text and lose the distinction the array store is built on.
        if let Some(at) = super::super::array::as_index(context, Value(key))
            && let Some(elements) = context.elements_at(slot)
        {
            // Past the end is absent, not an error: `[1,2][9]` is `undefined`.
            // E um BURACO lido também é `undefined` — `[,1][0]` responde
            // `undefined` mesmo com a posição ausente. É `in` que os separa.
            let held = elements.get(at).copied();
            let answer = match held {
                Some(held) => super::super::array::visible(context, held),
                None => undefined_of(context),
            };
            return super::super::accessor::Found::Value(answer);
        }
        // A typed array's element, which is a byte range rather than a slot in
        // an element vector. Asked here for the reason the array branch is
        // asked before `ToPropertyKey`: the index is a number, and converting
        // it to text loses what the view is addressed by.
        //
        // An index past the end answers `undefined` and does NOT fall through
        // to a property — `new Uint8Array(2)[9]` is absent, not a lookup for
        // the name "9".
        if let Some(answer) = super::super::buffers::indexed_get(context, slot, Value(key)) {
            return super::super::accessor::Found::Value(answer);
        }
        if let Some(answer) = string_element(context, slot, Value(key)) {
            return super::super::accessor::Found::Value(answer);
        }
        let Some(key) = property_key(context, Value(key)) else {
            return super::super::accessor::Found::Value(undefined_of(context));
        };
        if let Some(answer) = string_property(context, slot, key) {
            return super::super::accessor::Found::Value(answer);
        }
        // Through the accessor-aware walk, not `read_property`, and the reason
        // is the whole point of a computed read: `o[k]` and `o.x` name the same
        // property, so one of them finding a getter and the other reading a
        // slot would make which spelling was written decide what a property IS.
        super::super::accessor::resolve(context, slot, key)
    });
    match found {
        super::super::accessor::Found::Value(value) => value,
        super::super::accessor::Found::Getter(getter) => {
            let undefined = with_current(|context| undefined_of(context));
            super::super::functions::call(getter, object, undefined, undefined, undefined, undefined)
        }
        super::super::accessor::Found::Absent => with_current(|context| undefined_of(context)),
    }
}

/// `object[key] = value`. Answers the value, because an assignment is an
/// expression.
///
/// Two statements, like the named write: a setter is user code and runs after
/// the borrow ends.
#[rtse::entry]
pub fn set_indexed(object: u64, key: u64, value: u64) -> u64 {
    // Ver a nota em [`get_indexed`]: a conversão da chave só acontece quando há
    // um proxy para perguntar, ou quando a chave é um objeto.
    let (key, trap) = opened(object, key);
    if let Some(named) = trap
        && let Some(answered) = super::super::proxy::set(object, named, value)
    {
        return answered;
    }
    let setter = with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            return None;
        };
        if let Some(at) = super::super::array::as_index(context, Value(key))
            && let Some(elements) = context.elements_at_mut(slot)
        {
            // Writing past the end grows the array and fills the gap with
            // `undefined`, which is what the language does — `let a = []; a[2]
            // = 1` leaves length 3. Holes are `undefined` here rather than a
            // distinct absent-ness, which is a stated gap: `0 in [,1]` is
            // false and this cannot say so.
            //
            // Filled by the resize itself. It used to resize with `0` and then
            // scan the WHOLE vector rewriting every element equal to `0` into
            // `undefined` — and `0` is the bit pattern of `+0.0`, a genuine
            // double. So `a[0] = 0; a[2] = 1;` turned `a[0]` into `undefined`:
            // a stored value destroyed by a later write somewhere else, which
            // is the worst shape a wrong answer takes. There is no scan now.
            let cresceu = at >= elements.len();
            if cresceu {
                let wanted = at + 1;
                // As posições que o salto pula são BURACOS, não `undefined`
                // armazenados: `const a = []; a[2] = 1` deixa `0 in a` falso.
                let absent = super::super::array::hole_of(context);
                let elements = context
                    .elements_at_mut(slot)
                    .expect("the array was just found");
                elements.resize(wanted, absent);
            }
            let elements = context
                .elements_at_mut(slot)
                .expect("the array was just found");
            elements[at] = value;
            // `length` é uma propriedade que os dois caminhos leem, então
            // CRESCER tem de escrevê-la — código compilado lê a armazenada e
            // nunca pergunta ao runtime num acerto.
            //
            // Só ao crescer. Escrevê-la em toda atribuição era trabalho morto no
            // laço mais comum que existe (`a[i] = v` dentro de um `for`): o
            // comprimento não mudou, e a escrita ainda assim resolvia a chave e
            // percorria a shape.
            if cresceu {
                let count = elements.len();
                super::super::array::set_length(context, slot, count);
            }
            return None;
        }
        // A typed array's element. Answering true means the write landed in the
        // view's bytes; a write past the end is DROPPED rather than falling
        // through to a property, which is what the language does — a typed
        // array does not grow and `a[9] = 1` on a two-element one stores
        // nothing anybody can read back.
        if super::super::buffers::indexed_set(context, slot, Value(key), value) {
            return None;
        }
        let Some(key) = property_key(context, Value(key)) else {
            return None;
        };
        // The same question the named write asks, and it has to be asked here
        // too: `o[k] = v` and `o.x = v` reach one property, so a setter found
        // by one spelling and a slot written by the other is two answers to
        // what that property IS.
        if let Some(setter) = super::super::accessor::setter_for(context, slot, key) {
            return Some(setter);
        }
        put(context, slot, key, value);
        None
    });
    if let Some(setter) = setter {
        let undefined = with_current(|context| undefined_of(context));
        super::super::functions::call(setter, object, value, undefined, undefined, undefined);
    }
    value
}

