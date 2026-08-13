//! The individual checks.
//!
//! Each function here answers one question about a function and appends what it
//! finds. They are kept separate so that a rule can be read, and argued with, on
//! its own.

use std::collections::{HashMap, HashSet};

use super::error::VerifyError;
use crate::ir::{BlockCall, BlockId, Function, Inst, InstId, Terminator, ValueId};
use crate::repr::Repr;
use crate::types::TypeRegistry;
use crate::unwind::RegionId;

/// Every block ends somewhere.
///
/// A block without a terminator is not an incomplete program, it is an
/// unanswerable one: control reaches its end and there is nowhere to go.
pub(super) fn check_blocks_terminate(func: &Function, errors: &mut Vec<VerifyError>) {
    for (id, block) in func.blocks() {
        if block.terminator.is_none() {
            errors.push(VerifyError::UnterminatedBlock(id));
        }
    }
}

/// Branch targets exist, receive what they declare, and guards are well formed.
pub(super) fn check_terminators(func: &Function, errors: &mut Vec<VerifyError>) {
    for (from, block) in func.blocks() {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        match terminator {
            Terminator::Jump(call) => check_block_call(func, from, call, 0, errors),

            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                let repr = func.repr_of(*cond);
                if repr != Repr::Bool {
                    errors.push(VerifyError::ConditionNotBool { from, found: repr });
                }
                check_block_call(func, from, then_block, 0, errors);
                check_block_call(func, from, else_block, 0, errors);
            }

            Terminator::Guard {
                input,
                expect,
                ok,
                fail,
            } => {
                check_guard(func, from, *input, *expect, ok, errors);
                check_block_call(func, from, fail, 0, errors);
            }

            Terminator::GuardType {
                object,
                expect,
                ok,
                fail,
            } => {
                check_type_guard(func, from, *object, *expect, ok, errors);
                check_block_call(func, from, fail, 0, errors);
            }

            Terminator::CachedSet {
                object,
                value,
                hit,
                miss,
                ..
            } => {
                // The object must be a proven reference, for the same reason a
                // cached read's must: the address is computed from it and a
                // number is not an address.
                let found = func.repr_of(*object);
                if !matches!(found, Repr::Ref(_)) {
                    errors.push(VerifyError::GuardTypeOnNonReference { from, found });
                }
                // What is stored is a JavaScript value. A proven double written
                // into a slot a later read takes as generic would be read as
                // whatever its bits mean, which is the widening the builder
                // inserts rather than a check anything can make here.
                let written = func.repr_of(*value);
                if written != Repr::Tagged {
                    errors.push(VerifyError::GuardTypeOnNonReference {
                        from,
                        found: written,
                    });
                }
                // No narrowed parameter: a store produces nothing.
                check_block_call(func, from, hit, 0, errors);
                check_block_call(func, from, miss, 0, errors);
            }

            // One check for both forms, and deliberately so: what the verifier
            // has to reject is identical — the object is a reference, the cache
            // names a site this function declared, and the hit block takes the
            // value it will be handed. Where the two differ is what the LOWERING
            // emits, which is not a well-formedness question. A second copy of
            // these four checks would be a second place for them to drift.
            Terminator::CachedGet {
                object,
                cache,
                hit,
                miss,
                ..
            }
            | Terminator::CachedGetIndirect {
                object,
                cache,
                hit,
                miss,
                ..
            } => {
                check_cached_get(func, from, *object, *cache, hit, errors);
                check_block_call(func, from, miss, 0, errors);
            }

            Terminator::Throw { payload, .. } => {
                let repr = func.repr_of(*payload);
                if repr != Repr::Tagged {
                    errors.push(VerifyError::ThrownValueNotGeneric { from, found: repr });
                }
            }

            Terminator::TailCall { .. } | Terminator::TailCallIndirect { .. } => {}

            Terminator::Return(_) | Terminator::CleanupDone | Terminator::Trap(_) => {}
        }
    }
}

/// Regions name blocks that exist, and handlers receive what they catch.
pub(super) fn check_unwind(func: &Function, errors: &mut Vec<VerifyError>) {
    for index in 0..func.regions.len() {
        let id = RegionId(index as u32);
        let region = func.regions.get(id).expect("index is within the tree");

        if let Some(cleanup) = region.cleanup
            && func.block(cleanup).is_none()
        {
            errors.push(VerifyError::UnknownRegionBlock {
                region: id,
                target: cleanup,
            });
        }

        for handler in &region.handlers {
            let Some(target) = func.block(handler.block) else {
                errors.push(VerifyError::UnknownRegionBlock {
                    region: id,
                    target: handler.block,
                });
                continue;
            };

            let receives = target.params.first().map(|&p| func.repr_of(p));
            if receives != Some(Repr::Tagged) {
                errors.push(VerifyError::HandlerMissingPayload {
                    region: id,
                    target: handler.block,
                });
            }
        }
    }

    for (block, _) in func.blocks() {
        if let Some(region) = func.region_of(block)
            && func.regions.get(region).is_none()
        {
            errors.push(VerifyError::UnknownRegion { block, region });
        }
    }

    check_cleanups(func, errors);
}

/// A cleanup is a piece with one entry and one exit, and reads only itself.
///
/// All three exist for one reason: a cleanup is not jumped to, it is copied into
/// each path that unwinds through it. A second exit would make the copy ambiguous,
/// parameters would have to come from paths with nothing in common, and reading a
/// value from outside would read something that does not exist where the copy
/// lands.
///
/// # Why the piece is several blocks and used to be one
///
/// Nothing a language runs on the way out fits in one block. `x + y` alone
/// emits a fast path and a slow one, and a disposer is a call — so a cleanup
/// limited to a single straight line could hold arithmetic on proven operands
/// and nothing else, which left `finally` and `using` with no cleanup they
/// could be.
///
/// The three rules did not change with it, because none of them ever depended
/// on the piece being one block. One *entry*: the region names one, and nothing
/// else may jump into the piece. One *exit*: every `CleanupDone` in the piece
/// leaves to the same place, so several are still one exit. Reads only itself:
/// now over everything the piece defines, including its own block parameters,
/// which is what lets a merge inside a cleanup work at all.
fn check_cleanups(func: &Function, errors: &mut Vec<VerifyError>) {
    let mut cleanups = Vec::new();
    for index in 0..func.regions.len() {
        let id = RegionId(index as u32);
        if let Some(region) = func.regions.get(id)
            && let Some(cleanup) = region.cleanup
        {
            cleanups.push((id, cleanup));
        }
    }

    let mut inside_some_cleanup = Vec::new();
    for &(region, entry) in &cleanups {
        let piece = cleanup_piece(func, entry);
        inside_some_cleanup.extend(piece.iter().copied());

        // The entry alone. An interior block's parameters come from within the
        // piece, where there is something to pass them; the entry's would have
        // to come from every path that unwinds through it, and those have
        // nothing in common.
        if let Some(data) = func.block(entry)
            && !data.params.is_empty()
        {
            errors.push(VerifyError::CleanupTakesParameters { block: entry });
        }

        // Everything the piece defines, and everything the function's entry
        // block defines.
        //
        // The entry is the relaxation the copy needed to be useful. Anything a
        // cleanup does with a variable reads the environment pointer, which is
        // established once at function entry — so a cleanup that could read
        // only itself could touch no variable, and a `finally` that touches no
        // variable is not one anybody writes.
        //
        // It is sound for the reason the original rule was: a copy must not
        // read something that does not exist where it lands. The entry block
        // dominates every block in the function, so its values exist at every
        // point that can unwind. Dominance in general would admit more, and
        // needs an analysis this does not have; the entry is the part of it
        // that is free and covers the case.
        // A `HashSet`, not a `Vec`: every use below is `.contains(&operand)` —
        // membership only, never iterated to produce output — so rule 13 does
        // not constrain this collection at all. It used to be a `Vec` probed
        // with `.contains`, which is a linear scan repeated for every operand
        // of every instruction in the piece: O(pieces x instructions x
        // defined-values). The `try`/`finally` measurement (60 statements,
        // 29.32 ms, ~4x the per-statement cost of 300 `if`/`else` at 7.55 ms)
        // is this shape — a scan inside a loop that runs once per operand.
        let mut defined: HashSet<ValueId> = HashSet::new();
        if let Some(data) = func.block(func.entry) {
            defined.extend(data.params.iter().copied());
            defined.extend(
                data.insts
                    .iter()
                    .filter_map(|&i| func.inst(i))
                    .flat_map(|d| d.results.iter().copied()),
            );
        }
        for &block in &piece {
            let Some(data) = func.block(block) else {
                continue;
            };
            defined.extend(data.params.iter().copied());
            defined.extend(
                data.insts
                    .iter()
                    .filter_map(|&i| func.inst(i))
                    .flat_map(|d| d.results.iter().copied()),
            );
        }

        let mut leaves = false;
        for &block in &piece {
            let Some(data) = func.block(block) else {
                continue;
            };

            for &inst in &data.insts {
                let Some(inst_data) = func.inst(inst) else {
                    continue;
                };
                for operand in inst_data.inst.operands() {
                    if !defined.contains(&operand) {
                        errors.push(VerifyError::CleanupReadsOutsideItself {
                            block,
                            value: operand,
                        });
                    }
                }
            }

            match &data.terminator {
                Some(Terminator::CleanupDone) => leaves = true,
                // Anything that branches stays in the piece by construction —
                // that is how the piece was collected, and a guard and an
                // inline cache branch as much as a jump does. What is rejected
                // is a terminator with no successor at all: a return, a throw
                // or a tail call leaves the copy through a path the unwinding
                // it is part of knows nothing about.
                Some(terminator) if !terminator.successors().is_empty() => {}
                _ => errors.push(VerifyError::CleanupDoesNotEnd { region, block }),
            }
        }

        if !leaves {
            errors.push(VerifyError::CleanupDoesNotEnd {
                region,
                block: entry,
            });
        }
    }

    // Only a cleanup ends that way. Anywhere else it would be a block claiming
    // to hand control back to an unwind that is not happening.
    for (block, data) in func.blocks() {
        if matches!(data.terminator, Some(Terminator::CleanupDone))
            && !inside_some_cleanup.contains(&block)
        {
            errors.push(VerifyError::CleanupEndOutsideCleanup { block });
        }
    }
}

/// The blocks one cleanup is made of.
///
/// Reachable from the entry without leaving through a `CleanupDone`. Derived
/// rather than declared, for the reason rule 8 gives generally: a piece a client
/// listed could disagree with the piece control actually reaches, and the copy
/// would then omit a block the original runs.
fn cleanup_piece(func: &Function, entry: BlockId) -> Vec<BlockId> {
    let mut order = Vec::new();
    // `seen` exists purely as a membership index alongside `order`: the
    // worklist can revisit a block through more than one predecessor, and
    // `order.contains(&id)` here was a linear scan of a `Vec` that grows with
    // every block already collected — O(n^2) in the blocks of one cleanup.
    // Safe as a `HashSet` under rule 13 because `order` is sorted below
    // regardless of the order blocks were popped from the worklist in, so
    // hash-iteration order is never observed — only `seen.contains` is used,
    // never `seen`'s own iteration.
    let mut seen: HashSet<BlockId> = HashSet::new();
    let mut queue = vec![entry];
    while let Some(id) = queue.pop() {
        if !seen.insert(id) || func.block(id).is_none() {
            continue;
        }
        order.push(id);
        // Every successor, not the two obvious ones. A guard and an inline
        // cache are branches too, and a piece that followed only jumps would
        // stop at the first arithmetic on unproven operands — which is most
        // cleanups, and which reads as "does not end" rather than as a missing
        // edge.
        if let Some(terminator) = func.block(id).and_then(|d| d.terminator.as_ref()) {
            queue.extend(terminator.successors());
        }
    }
    order.sort();
    order
}

/// A cached read reads an object and hands back what it holds.
///
/// Three things, each for the same reason the other guards check theirs: the
/// subject has to be a reference, because a layout is read out of an object; the
/// value arrives as a block parameter, so it exists only where it was found; and
/// it arrives generic, because what a property holds is not known where its
/// layout is not.
fn check_cached_get(
    func: &Function,
    from: BlockId,
    object: ValueId,
    cache: crate::ir::CacheId,
    hit: &BlockCall,
    errors: &mut Vec<VerifyError>,
) {
    let found = func.repr_of(object);
    if !matches!(found, Repr::Ref(_)) {
        errors.push(VerifyError::GuardTypeOnNonReference { from, found });
    }
    if cache.index() >= func.cache_count() {
        errors.push(VerifyError::UnknownCache { from, cache });
    }

    let Some(target) = func.block(hit.block) else {
        errors.push(VerifyError::UnknownBlock {
            from,
            target: hit.block,
        });
        return;
    };

    if target.params.first().map(|&p| func.repr_of(p)) != Some(Repr::Tagged) {
        errors.push(VerifyError::GuardTargetMissingValue {
            from,
            target: hit.block,
            expect: Repr::Tagged,
        });
        return;
    }

    check_block_call(func, from, hit, 1, errors);
}

/// A type guard reads an object and hands it back narrowed.
///
/// The object has to be a reference already: reading a type out of something that
/// is not an object reads whatever happens to be at that address. Establishing
/// that much is what the other guard is for, so the two compose rather than one
/// doing both badly.
fn check_type_guard(
    func: &Function,
    from: BlockId,
    object: ValueId,
    expect: crate::types::TypeId,
    ok: &BlockCall,
    errors: &mut Vec<VerifyError>,
) {
    let found = func.repr_of(object);
    if !matches!(found, Repr::Ref(_)) {
        errors.push(VerifyError::GuardTypeOnNonReference { from, found });
    }

    let Some(target) = func.block(ok.block) else {
        errors.push(VerifyError::UnknownBlock {
            from,
            target: ok.block,
        });
        return;
    };

    let narrowed = target.params.first().map(|&p| func.repr_of(p));
    let wanted = Repr::Ref(crate::repr::RefKind::Aggregate(expect));
    if narrowed != Some(wanted) {
        errors.push(VerifyError::GuardTargetMissingValue {
            from,
            target: ok.block,
            expect: wanted,
        });
        return;
    }

    check_block_call(func, from, ok, 1, errors);
}

/// A guard tests something generic and hands the narrowed value to its success
/// block as that block's first parameter.
///
/// Both halves matter. Testing a value that is already proven means either the
/// guard is dead or the representation is wrong. And the narrowed value arriving
/// as a parameter is what confines it to the path where the test held — if the
/// success block did not receive it, nothing would stop the narrowed value being
/// used where the test failed.
fn check_guard(
    func: &Function,
    from: BlockId,
    input: ValueId,
    expect: Repr,
    ok: &BlockCall,
    errors: &mut Vec<VerifyError>,
) {
    let input_repr = func.repr_of(input);
    if input_repr != Repr::Tagged {
        errors.push(VerifyError::GuardOnProvenValue {
            from,
            found: input_repr,
        });
    }

    let Some(target) = func.block(ok.block) else {
        errors.push(VerifyError::UnknownBlock {
            from,
            target: ok.block,
        });
        return;
    };

    let narrowed = target.params.first().map(|&p| func.repr_of(p));
    if narrowed != Some(expect) {
        errors.push(VerifyError::GuardTargetMissingValue {
            from,
            target: ok.block,
            expect,
        });
        return;
    }

    // The guard supplies the first parameter, so the explicit arguments start at
    // the second.
    check_block_call(func, from, ok, 1, errors);
}

/// A branch's arguments match the parameters they bind, starting at `skip`.
fn check_block_call(
    func: &Function,
    from: BlockId,
    call: &BlockCall,
    skip: usize,
    errors: &mut Vec<VerifyError>,
) {
    let Some(target) = func.block(call.block) else {
        errors.push(VerifyError::UnknownBlock {
            from,
            target: call.block,
        });
        return;
    };

    let params = &target.params[skip.min(target.params.len())..];
    if params.len() != call.args.len() {
        errors.push(VerifyError::ArgumentCount {
            from,
            target: call.block,
            expected: params.len(),
            found: call.args.len(),
        });
        return;
    }

    for (position, (&param, &arg)) in params.iter().zip(&call.args).enumerate() {
        let expected = func.repr_of(param);
        let found = func.repr_of(arg);
        if expected != found {
            errors.push(VerifyError::ArgumentRepr {
                from,
                target: call.block,
                position: position + skip,
                expected,
                found,
            });
        }
    }
}

/// Instructions apply to the representations they are defined for.
pub(super) fn check_instructions(
    func: &Function,
    types: &TypeRegistry,
    errors: &mut Vec<VerifyError>,
) {
    let narrowing_allowed = guard_success_blocks(func);

    for (block_id, block) in func.blocks() {
        for &inst_id in &block.insts {
            let Some(data) = func.inst(inst_id) else {
                continue;
            };
            match &data.inst {
                Inst::IntArith(_, a, b) => {
                    check_proven_pair(func, inst_id, *a, *b, Domain::Integer, errors);
                }
                Inst::FloatArith(_, a, b) => {
                    check_proven_pair(func, inst_id, *a, *b, Domain::Float, errors);
                }
                Inst::Bitwise(_, a, b) => {
                    check_proven_pair(func, inst_id, *a, *b, Domain::Integer, errors);
                }
                Inst::Compare(_, a, b) => {
                    check_proven_pair(func, inst_id, *a, *b, Domain::Any, errors);
                }

                // Rule 7: the builder refuses a non-float operand where the
                // mistake is made, and this refuses it for a representation
                // that did not come from the builder at all.
                Inst::ToInt32(value) => {
                    if func.repr_of(*value) != Repr::F64 {
                        errors.push(VerifyError::WrongDomain {
                            inst: inst_id,
                            found: func.repr_of(*value),
                        });
                    }
                }

                Inst::FloatUnary(_, value) => {
                    if func.repr_of(*value) != Repr::F64 {
                        errors.push(VerifyError::WrongDomain {
                            inst: inst_id,
                            found: func.repr_of(*value),
                        });
                    }
                }

                Inst::ToF64(value) => {
                    if func.repr_of(*value) != Repr::I32 {
                        errors.push(VerifyError::WrongDomain {
                            inst: inst_id,
                            found: func.repr_of(*value),
                        });
                    }
                }

                Inst::Widen(value) => {
                    if func.repr_of(*value) == Repr::Tagged {
                        errors.push(VerifyError::WrongDomain {
                            inst: inst_id,
                            found: Repr::Tagged,
                        });
                    }
                }

                Inst::Narrow(value, _) => {
                    if !narrowing_allowed.contains_key(&block_id) {
                        errors.push(VerifyError::UnguardedNarrowing { inst: inst_id });
                    }
                    if func.repr_of(*value) != Repr::Tagged {
                        errors.push(VerifyError::WrongDomain {
                            inst: inst_id,
                            found: func.repr_of(*value),
                        });
                    }
                }

                Inst::Generic(_, a, b) => {
                    for &operand in [a, b] {
                        let repr = func.repr_of(operand);
                        if repr != Repr::Tagged {
                            errors.push(VerifyError::WrongDomain {
                                inst: inst_id,
                                found: repr,
                            });
                        }
                    }
                }

                Inst::FieldLoad { ty, field, .. } => {
                    check_field(types, inst_id, *ty, *field, errors);
                }

                Inst::FieldStore {
                    ty, field, value, ..
                } => {
                    if let Some(expected) = check_field(types, inst_id, *ty, *field, errors) {
                        let found = func.repr_of(*value);
                        if expected != found {
                            errors.push(VerifyError::WrongDomain {
                                inst: inst_id,
                                found,
                            });
                        }
                    }
                }

                Inst::Alloc { ty, .. } => {
                    if !types.contains(*ty) {
                        errors.push(VerifyError::ForeignType {
                            inst: inst_id,
                            ty: *ty,
                        });
                    }
                }

                Inst::Suspend | Inst::Await { .. } => {
                    if !func.signature.may_suspend {
                        errors.push(VerifyError::UndeclaredSuspension { inst: inst_id });
                    }
                }

                Inst::PromiseSettle { value, .. } => {
                    let repr = func.repr_of(*value);
                    if repr != Repr::Tagged {
                        errors.push(VerifyError::WrongDomain {
                            inst: inst_id,
                            found: repr,
                        });
                    }
                }

                // Calls have their own pass: they are the only instructions that
                // consult a second registry, and four separate facts have to
                // agree at once.
                // Calls have their own pass, and `FuncAddr` is checked there
                // too: all three consult the function registry, which this
                // pass does not hold.
                Inst::Call { .. } | Inst::CallIndirect { .. } | Inst::FuncAddr { .. } => {}

                Inst::Const(_) | Inst::PromiseNew => {}
            }
        }
    }
}

/// Which representation domain an operation applies to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Domain {
    Integer,
    Float,
    Any,
}

/// Two operands are proven, agree with each other, and suit the domain.
fn check_proven_pair(
    func: &Function,
    inst: InstId,
    a: ValueId,
    b: ValueId,
    domain: Domain,
    errors: &mut Vec<VerifyError>,
) {
    let left = func.repr_of(a);
    let right = func.repr_of(b);

    if left == Repr::Tagged || right == Repr::Tagged {
        errors.push(VerifyError::GenericOperand { inst });
        return;
    }
    if left != right {
        errors.push(VerifyError::MixedOperands { inst, left, right });
        return;
    }

    let suits = match domain {
        Domain::Integer => left.is_integer(),
        Domain::Float => left.is_float(),
        Domain::Any => true,
    };
    if !suits {
        errors.push(VerifyError::WrongDomain { inst, found: left });
    }
}

/// The field exists; returns its representation when it does.
fn check_field(
    types: &TypeRegistry,
    inst: InstId,
    ty: crate::types::TypeId,
    field: u32,
    errors: &mut Vec<VerifyError>,
) -> Option<Repr> {
    if !types.contains(ty) {
        errors.push(VerifyError::ForeignType { inst, ty });
        return None;
    }
    match types.layout(ty).field(field as usize) {
        Some(layout) => Some(layout.repr),
        None => {
            errors.push(VerifyError::NoSuchField { inst, ty, field });
            None
        }
    }
}

/// Blocks reachable only through a guard's success edge, with what that guard
/// established.
fn guard_success_blocks(func: &Function) -> HashMap<BlockId, Repr> {
    let mut blocks = HashMap::new();
    for (_, block) in func.blocks() {
        if let Some(Terminator::Guard { expect, ok, .. }) = &block.terminator {
            blocks.insert(ok.block, *expect);
        }
    }
    blocks
}

/// Returns match the signature.
pub(super) fn check_returns(func: &Function, errors: &mut Vec<VerifyError>) {
    let expected = &func.signature.returns;
    for (from, block) in func.blocks() {
        let Some(Terminator::Return(values)) = &block.terminator else {
            continue;
        };

        if values.len() != expected.len() {
            errors.push(VerifyError::ReturnArity {
                from,
                expected: expected.len(),
                found: values.len(),
            });
            continue;
        }

        for (position, (&value, &want)) in values.iter().zip(expected).enumerate() {
            let found = func.repr_of(value);
            if found != want {
                errors.push(VerifyError::ReturnRepr {
                    from,
                    position,
                    expected: want,
                    found,
                });
            }
        }
    }
}
