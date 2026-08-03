//! The individual checks.
//!
//! Each function here answers one question about a function and appends what it
//! finds. They are kept separate so that a rule can be read, and argued with, on
//! its own.

use std::collections::HashMap;

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
        let Some(terminator) = &block.terminator else { continue };
        match terminator {
            Terminator::Jump(call) => check_block_call(func, from, call, 0, errors),

            Terminator::Branch { cond, then_block, else_block } => {
                let repr = func.repr_of(*cond);
                if repr != Repr::Bool {
                    errors.push(VerifyError::ConditionNotBool { from, found: repr });
                }
                check_block_call(func, from, then_block, 0, errors);
                check_block_call(func, from, else_block, 0, errors);
            }

            Terminator::Guard { input, expect, ok, fail } => {
                check_guard(func, from, *input, *expect, ok, errors);
                check_block_call(func, from, fail, 0, errors);
            }

            Terminator::Throw { payload, .. } => {
                let repr = func.repr_of(*payload);
                if repr != Repr::Tagged {
                    errors.push(VerifyError::ThrownValueNotGeneric { from, found: repr });
                }
            }

            Terminator::Return(_) | Terminator::Trap(_) => {}
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
            errors.push(VerifyError::UnknownRegionBlock { region: id, target: cleanup });
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
        errors.push(VerifyError::GuardOnProvenValue { from, found: input_repr });
    }

    let Some(target) = func.block(ok.block) else {
        errors.push(VerifyError::UnknownBlock { from, target: ok.block });
        return;
    };

    let narrowed = target.params.first().map(|&p| func.repr_of(p));
    if narrowed != Some(expect) {
        errors.push(VerifyError::GuardTargetMissingValue { from, target: ok.block, expect });
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
        errors.push(VerifyError::UnknownBlock { from, target: call.block });
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
            let Some(data) = func.inst(inst_id) else { continue };
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
                            errors.push(VerifyError::WrongDomain { inst: inst_id, found: repr });
                        }
                    }
                }

                Inst::FieldLoad { ty, field, .. } => {
                    check_field(types, inst_id, *ty, *field, errors);
                }

                Inst::FieldStore { ty, field, value, .. } => {
                    if let Some(expected) = check_field(types, inst_id, *ty, *field, errors) {
                        let found = func.repr_of(*value);
                        if expected != found {
                            errors.push(VerifyError::WrongDomain { inst: inst_id, found });
                        }
                    }
                }

                Inst::Alloc { ty, .. } => {
                    if !types.contains(*ty) {
                        errors.push(VerifyError::ForeignType { inst: inst_id, ty: *ty });
                    }
                }

                Inst::Suspend => {
                    if !func.signature.may_suspend {
                        errors.push(VerifyError::UndeclaredSuspension { inst: inst_id });
                    }
                }

                Inst::Const(_) => {}
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
        let Some(Terminator::Return(values)) = &block.terminator else { continue };

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
                errors.push(VerifyError::ReturnRepr { from, position, expected: want, found });
            }
        }
    }
}
