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

use crate::domain::{Js, JsConst, JsPrim};
use crate::values::Singleton;

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
        let facts = Facts::of(func, domain);
        let mut changed = false;
        for block in func.block_ids() {
            for inst in func.block(block).insts.clone() {
                let Op::Prim { prim, args } = &func.inst(inst).op else {
                    continue;
                };
                // A TRUTH VALUE over constants, before the integer arithmetic below: the
                // operators here take one operand as often as two.
                if let Some(truth) = fold_truth(domain, *prim, args, &facts) {
                    let at = &mut func.insts[inst.0 as usize];
                    at.op = Op::Const(Const::Bool(truth));
                    at.effect = rts_mir::Effect::PURE;
                    folded += 1;
                    changed = true;
                    continue;
                }
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

/// A constant the language names, as far as a comparison can read it: no object, no
/// symbol, no bigint -- nothing whose comparison could reach code the program wrote.
#[derive(Clone, Debug, PartialEq)]
enum Known {
    Num(f64),
    Bool(bool),
    Undefined,
    Null,
    Text(Vec<u16>),
}

impl Known {
    /// The language's truth rule over this value, the seven falsy cases included.
    fn truthy(&self) -> bool {
        match self {
            Known::Num(held) => !(*held == 0.0 || held.is_nan()),
            Known::Bool(held) => *held,
            Known::Undefined | Known::Null => false,
            Known::Text(units) => !units.is_empty(),
        }
    }

    fn nullish(&self) -> bool {
        matches!(self, Known::Undefined | Known::Null)
    }

    /// `ToNumber` where it is a number or a boolean; nothing else is read as one here.
    fn number(&self) -> Option<f64> {
        match self {
            Known::Num(held) => Some(*held),
            Known::Bool(held) => Some(f64::from(u8::from(*held))),
            _ => None,
        }
    }

    /// `===`: different kinds are never equal, and `NaN` is not equal to itself.
    fn strictly(&self, other: &Known) -> bool {
        match (self, other) {
            (Known::Num(one), Known::Num(two)) => one == two,
            (Known::Bool(one), Known::Bool(two)) => one == two,
            (Known::Text(one), Known::Text(two)) => one == two,
            (Known::Undefined, Known::Undefined) | (Known::Null, Known::Null) => true,
            _ => false,
        }
    }

    /// `==`, where the coercion is one of the cheap arms the specification lists first.
    /// A number against a string is `ToNumber` of the text, which is not folded.
    fn loosely(&self, other: &Known) -> Option<bool> {
        if self.nullish() || other.nullish() {
            return Some(self.nullish() && other.nullish());
        }
        match (self, other) {
            (Known::Text(_), Known::Text(_)) => Some(self.strictly(other)),
            (Known::Text(_), _) | (_, Known::Text(_)) => None,
            _ => Some(self.number()? == other.number()?),
        }
    }
}

/// What the constants of a function are, by the value that holds each.
struct Facts(BTreeMap<ValueId, Known>);

impl Facts {
    fn of(func: &Func, domain: &Js) -> Facts {
        let mut held = BTreeMap::new();
        for block in func.block_ids() {
            for inst in &func.block(block).insts {
                let inst = func.inst(*inst);
                let Op::Const(constant) = &inst.op else {
                    continue;
                };
                let known = match constant {
                    Const::Int(number) => Known::Num(*number as f64),
                    Const::Float(number) => Known::Num(*number),
                    Const::Bool(truth) => Known::Bool(*truth),
                    Const::Declared(index) => match domain.declared(*index) {
                        Some(JsConst::Singleton(Singleton::Undefined)) => Known::Undefined,
                        Some(JsConst::Singleton(Singleton::Null)) => Known::Null,
                        Some(JsConst::Text(text)) => Known::Text(text.units().to_vec()),
                        _ => continue,
                    },
                };
                held.insert(inst.result, known);
            }
        }
        Facts(held)
    }
}

/// The answer of a comparison or a truth test whose operands are all constants, or
/// `None` where it is not provably one.
///
/// Exactly the rows that cannot reach user code once no operand is an object: `===`,
/// `==` over the cheap arms, the four relational operators over numbers and booleans,
/// `!`, the truth test and the nullish test. Relational operators over text, and `==`
/// between a string and a number, are left to the runtime -- they are right there and
/// not worth a second implementation of `StringToNumber` and code-unit ordering here.
fn fold_truth(domain: &Js, prim: rts_mir::cfg::Prim, args: &[ValueId], facts: &Facts) -> Option<bool> {
    let which = domain.meaning(prim)?;
    let get = |held: &ValueId| facts.0.get(held);
    match (which, args) {
        (JsPrim::Truthy, [one]) => Some(get(one)?.truthy()),
        (JsPrim::Not, [one]) => Some(!get(one)?.truthy()),
        (JsPrim::IsNullish, [one]) => Some(get(one)?.nullish()),
        (JsPrim::StrictEquals, [one, two]) => Some(get(one)?.strictly(get(two)?)),
        (JsPrim::LooseEquals, [one, two]) => get(one)?.loosely(get(two)?),
        (
            JsPrim::LessThan | JsPrim::GreaterThan | JsPrim::LessOrEqual | JsPrim::GreaterOrEqual,
            [one, two],
        ) => {
            // `NaN` compares false under all four, which is what the host's partial
            // comparison answers too.
            let (one, two) = (get(one)?.number()?, get(two)?.number()?);
            Some(match which {
                JsPrim::LessThan => one < two,
                JsPrim::GreaterThan => one > two,
                JsPrim::LessOrEqual => one <= two,
                _ => one >= two,
            })
        }
        _ => None,
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
