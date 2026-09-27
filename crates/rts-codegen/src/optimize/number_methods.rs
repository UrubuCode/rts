//! A number's `toString` and `toFixed`, called directly where the inference proved
//! the receiver a number.
//!
//! `lower/intrinsic.rs::number_method_intrinsic` makes the same substitution at
//! lowering time, and there it can only see a receiver whose type the lowering
//! already knows -- an arithmetic result, a literal. A loop's COUNTER is only a
//! number once the back edge has been joined, which is after the lowering and
//! inside the inference; `i.toString()` in a loop is the commonest spelling there
//! is, and it stayed a call. So this runs after the inference, as `template.rs`
//! does and for the same reason.
//!
//! What it rewrites is exactly the shape `lower/calls.rs` emits for a method call:
//! a `FieldRead` of the receiver under the member's key, read once, and a `Call`
//! through that value with the receiver as its receiver. Where the receiver's type
//! is `Int32` or `Double`, the key spells one of the two members, the read has no
//! other reader and `Number` is the language's, the call becomes the direct entry
//! over the receiver and its one argument -- `undefined` where none was written --
//! and the read is dropped. The entry's body IS the member's (`rts-core`'s
//! `number/direct.rs`), so nothing about the answer changes.

use std::collections::{BTreeMap, BTreeSet};

use rts_mir::cfg::{Callee, Const, Func, Inst, InstId, Op, ValueId};
use rts_mir::infer::Types;

use crate::domain::{Js, JsConst, JsPrim, Type};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::values::Singleton;

/// The two member names, as the program's interner numbered them -- `None` where the
/// program never spelled one, in which case no call can name it either.
#[derive(Clone, Copy)]
pub struct NumberMembers {
    /// `toString`.
    pub to_string: Option<Name>,
    /// `toFixed`.
    pub to_fixed: Option<Name>,
}

/// Rewrites every admitted call, and says how many.
pub fn fuse_number_methods(
    func: &mut Func,
    domain: &Js,
    types: &Types<Type>,
    members: NumberMembers,
) -> usize {
    let mut defined: BTreeMap<ValueId, InstId> = BTreeMap::new();
    let mut readers: BTreeMap<ValueId, usize> = BTreeMap::new();
    for block in func.block_ids() {
        for &inst in &func.block(block).insts {
            defined.insert(func.inst(inst).result, inst);
            for read in func.reads(inst) {
                *readers.entry(read).or_default() += 1;
            }
        }
        if let Some(end) = &func.block(block).terminator {
            for read in end.reads() {
                *readers.entry(read).or_default() += 1;
            }
        }
    }
    let key_of = |func: &Func, value: ValueId| -> Option<Name> {
        let inst = *defined.get(&value)?;
        let Op::Const(Const::Declared(index)) = &func.inst(inst).op else {
            return None;
        };
        match domain.declared(*index)? {
            JsConst::Key(name) => Some(*name),
            _ => None,
        }
    };

    let mut dropped = BTreeSet::new();
    let mut rewritten = 0;
    for block in func.block_ids() {
        let insts = func.block(block).insts.clone();
        for (position, inst) in insts.iter().copied().enumerate() {
            let Op::Call {
                callee: Callee::Dynamic(held),
                receiver: Some(receiver),
                args,
                ..
            } = &func.inst(inst).op
            else {
                continue;
            };
            let (held, receiver, args) = (*held, *receiver, args.clone());
            if !matches!(types.of(receiver), Type::Int32 | Type::Double) || readers.get(&held) != Some(&1) {
                continue;
            }
            let Some(&read) = defined.get(&held) else {
                continue;
            };
            let Op::Prim { prim, args: of } = &func.inst(read).op else {
                continue;
            };
            if domain.meaning(*prim) != Some(JsPrim::FieldRead) {
                continue;
            }
            let [object, key] = of.as_slice() else {
                continue;
            };
            if *object != receiver {
                continue;
            }
            let Some(name) = key_of(func, *key) else {
                continue;
            };
            let door = match (Some(name), args.len()) {
                (to_string, 0 | 1) if to_string == members.to_string => RuntimeOp::NumberToStringDirect,
                (to_fixed, 1) if to_fixed == members.to_fixed => RuntimeOp::NumberToFixedDirect,
                _ => continue,
            };
            // The one argument, or `undefined` minted just before the call: a new
            // value the inference that follows will type.
            let argument = match args.first() {
                Some(&argument) => argument,
                None => {
                    let at = func.inst(inst).at;
                    let result = ValueId(func.values);
                    func.values += 1;
                    func.insts.push(Inst {
                        op: Op::Const(Const::Declared(Singleton::Undefined as u32)),
                        result,
                        at,
                        effect: rts_mir::Effect::PURE,
                    });
                    let minted = InstId(func.insts.len() as u32 - 1);
                    func.blocks[block.0 as usize].insts.insert(position, minted);
                    result
                }
            };
            let entry = domain.entry_point(door);
            if let Op::Call {
                callee,
                receiver: recv,
                args,
                ..
            } = &mut func.insts[inst.0 as usize].op
            {
                *callee = Callee::Entry(entry);
                *recv = None;
                *args = vec![receiver, argument];
            }
            dropped.insert(read);
            rewritten += 1;
        }
    }
    rts_mir::passes::unlist(func, &dropped);
    rewritten
}
