//! Arithmetic over two integer constants, done while compiling.
//!
//! `2 ** 3` in a loop asked the runtime for 8 on every pass: the operands are
//! constants, and nothing about the language makes the answer depend on anything
//! else. Folded only where the answer is EXACTLY what the operation computes and is
//! again an integer the lattice names: `+`, `-` and `*` with no overflow out of the
//! 32-bit range the lowering gives literals, and `**` with a non-negative exponent
//! whose power fits. Anything else -- a double, an overflow, a negative power, which
//! answers a fraction -- is left to the operation.

use std::collections::BTreeMap;

use rts_mir::cfg::{Const, Func, Op, ValueId};

use crate::domain::{Js, JsPrim};

/// Folds every foldable operation, repeatedly, and says how many.
pub fn fold_constants(func: &mut Func, domain: &Js) -> usize {
    let mut folded = 0;
    loop {
        let known: BTreeMap<ValueId, i64> = func
            .block_ids()
            .flat_map(|block| func.block(block).insts.clone())
            .filter_map(|inst| match func.inst(inst).op {
                Op::Const(Const::Int(held)) => Some((func.inst(inst).result, held)),
                _ => None,
            })
            .collect();
        let mut changed = false;
        for block in func.block_ids() {
            for inst in func.block(block).insts.clone() {
                let Op::Prim { prim, args } = &func.inst(inst).op else {
                    continue;
                };
                let [left, right] = args.as_slice() else {
                    continue;
                };
                let (Some(left), Some(right)) = (known.get(left), known.get(right)) else {
                    continue;
                };
                let answer = match domain.meaning(*prim) {
                    Some(JsPrim::Add) => left.checked_add(*right),
                    Some(JsPrim::Subtract) => left.checked_sub(*right),
                    Some(JsPrim::Multiply) => left.checked_mul(*right),
                    Some(JsPrim::Exponent) => u32::try_from(*right)
                        .ok()
                        .and_then(|power| left.checked_pow(power)),
                    _ => None,
                };
                // `0 * -1` is `-0` in the language, which no integer is.
                let negative_zero = domain.meaning(*prim) == Some(JsPrim::Multiply)
                    && answer == Some(0)
                    && (*left < 0 || *right < 0);
                let Some(answer) = answer.filter(|held| i32::try_from(*held).is_ok()) else {
                    continue;
                };
                if negative_zero {
                    continue;
                }
                let at = &mut func.insts[inst.0 as usize];
                at.op = Op::Const(Const::Int(answer));
                at.effect = rts_mir::Effect::PURE;
                folded += 1;
                changed = true;
            }
        }
        if !changed {
            return folded;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(source: &str) -> (Func, Js) {
        let mut names = crate::names::Names::new();
        let program = crate::parse::parse_script(source, &mut names).expect("parses");
        let resolution = crate::names::resolve::resolve_module(&program.body);
        let function = program
            .body
            .iter()
            .find_map(|item| match item {
                crate::syntax::ModuleItem::Stmt(crate::syntax::Stmt {
                    kind: crate::syntax::StmtKind::Function(function),
                    ..
                }) => Some(function),
                _ => None,
            })
            .expect("one function");
        let lowered = crate::lower::lower(function, &resolution, rts_mir::Tier::Generic).expect("lowers");
        (lowered.func, lowered.domain)
    }

    /// `2 ** 3 + 1` is the constant 9, and an overflow or a negative power is not folded.
    #[test]
    fn integer_arithmetic_over_constants_is_its_answer() {
        let (mut func, domain) = graph("function f() { return 2 ** 3 + 1; }");
        assert_eq!(fold_constants(&mut func, &domain), 2);
        assert_eq!(rts_mir::verify(&func), Ok(()));
        for source in [
            "function f() { return 2 ** -1; }",
            "function f() { return 65536 * 65536; }",
            "function f() { return 0 * -1; }",
        ] {
            let (mut func, domain) = graph(source);
            assert_eq!(fold_constants(&mut func, &domain), 0, "{source}");
        }
    }
}
