//! The rules that govern calls.
//!
//! Kept apart from the rest because they are the only rules that consult a
//! second registry, and because a call is where three separate facts have to
//! agree at once: the shape, the convention, and where the frame is when
//! control leaves.
//!
//! # There is no rule about calling something that can suspend
//!
//! An earlier draft required a caller to declare it may park before calling a
//! callee that can. That rule contradicts the model this layer actually
//! implements. Suspension here is stackless: a function that can park returns a
//! promise, and the caller parks — if it parks at all — at its own await. A
//! plain function calling one that suspends simply receives a promise it does
//! not wait on, which is ordinary and not an error.
//!
//! The rule is written down as absent rather than silently dropped, because a
//! reader who assumes suspension is transitive will otherwise re-add it.

use super::error::{CallSite, VerifyError};
use crate::ir::{FuncRegistry, Function, Inst, SigId, Signature, Terminator, ValueId};
use crate::repr::{RefKind, Repr};

/// Checks every call and tail call in a function.
pub(super) fn check_calls(func: &Function, funcs: &FuncRegistry, errors: &mut Vec<VerifyError>) {
    for (block_id, block) in func.blocks() {
        for &inst_id in &block.insts {
            let Some(data) = func.inst(inst_id) else {
                continue;
            };
            let at = CallSite::Inst(inst_id);

            match &data.inst {
                Inst::Call { callee, args } => {
                    let Some(sig) = funcs.signature_of(*callee) else {
                        errors.push(VerifyError::UnknownCallee { at });
                        continue;
                    };
                    check_arguments(func, at, sig, args, errors);
                    check_results(func, inst_id, sig, &data.results, errors);
                }

                // Not a call, and checked here anyway: it is the third
                // instruction that consults the function registry, and the
                // question it asks — "was this callee declared" — is this
                // pass's question rather than the representation pass's. An id
                // naming nothing becomes an address naming nothing, which
                // fails at placement or, worse, does not.
                Inst::FuncAddr { callee } => {
                    if funcs.signature_of(*callee).is_none() {
                        errors.push(VerifyError::UnknownCallee { at });
                    }
                }

                Inst::CallIndirect { callee, sig, args } => {
                    check_callable(func, at, *callee, errors);
                    let Some(sig) = funcs.signature(*sig) else {
                        errors.push(VerifyError::UnknownCallee { at });
                        continue;
                    };
                    check_arguments(func, at, sig, args, errors);
                    check_results(func, inst_id, sig, &data.results, errors);
                }

                _ => {}
            }
        }

        match &block.terminator {
            Some(Terminator::TailCall { callee, args }) => {
                let at = CallSite::Tail(block_id);
                let Some(sig) = funcs.signature_of(*callee) else {
                    errors.push(VerifyError::UnknownCallee { at });
                    continue;
                };
                check_arguments(func, at, sig, args, errors);
                check_tail_position(func, block_id, sig, errors);
            }

            Some(Terminator::TailCallIndirect { callee, sig, args }) => {
                let at = CallSite::Tail(block_id);
                check_callable(func, at, *callee, errors);
                let Some(sig) = funcs.signature(*sig) else {
                    errors.push(VerifyError::UnknownCallee { at });
                    continue;
                };
                check_arguments(func, at, sig, args, errors);
                check_tail_position(func, block_id, sig, errors);
            }

            _ => {}
        }
    }
}

/// Arguments match the parameters they fill, in count and representation.
fn check_arguments(
    func: &Function,
    at: CallSite,
    sig: &Signature,
    args: &[ValueId],
    errors: &mut Vec<VerifyError>,
) {
    if sig.params.len() != args.len() {
        errors.push(VerifyError::CallArity {
            at,
            expected: sig.params.len(),
            found: args.len(),
        });
        return;
    }

    for (position, (&arg, &expected)) in args.iter().zip(&sig.params).enumerate() {
        let found = func.repr_of(arg);
        if found != expected {
            errors.push(VerifyError::CallArgumentRepr {
                at,
                position,
                expected,
                found,
            });
        }
    }
}

/// A call binds exactly what its callee returns.
fn check_results(
    func: &Function,
    inst: crate::ir::InstId,
    sig: &Signature,
    results: &[ValueId],
    errors: &mut Vec<VerifyError>,
) {
    if sig.returns.len() != results.len() {
        errors.push(VerifyError::CallResultArity {
            inst,
            expected: sig.returns.len(),
            found: results.len(),
        });
        return;
    }

    for (position, (&result, &expected)) in results.iter().zip(&sig.returns).enumerate() {
        let found = func.repr_of(result);
        if found != expected {
            errors.push(VerifyError::CallArgumentRepr {
                at: CallSite::Inst(inst),
                position,
                expected,
                found,
            });
        }
    }
}

/// An indirect callee is code, proven rather than assumed.
fn check_callable(func: &Function, at: CallSite, callee: ValueId, errors: &mut Vec<VerifyError>) {
    let found = func.repr_of(callee);
    if found != Repr::Ref(RefKind::Callable) {
        errors.push(VerifyError::IndirectCalleeNotCallable { at, found });
    }
}

/// A tail call is legal here at all.
fn check_tail_position(
    func: &Function,
    block: crate::ir::BlockId,
    callee: &Signature,
    errors: &mut Vec<VerifyError>,
) {
    if !func.signature.permits_tail_call_to(callee) {
        errors.push(VerifyError::TailCallNotPermitted { from: block });
    }
    if func.region_of(block).is_some() {
        errors.push(VerifyError::TailCallInProtectedRegion { from: block });
    }
}

/// Whether an indirect call's shape was issued by this registry.
///
/// Exposed so that a client can ask before building rather than after, in the
/// one case where the builder cannot answer from the function alone.
pub fn knows_signature(funcs: &FuncRegistry, sig: SigId) -> bool {
    funcs.signature(sig).is_some()
}
