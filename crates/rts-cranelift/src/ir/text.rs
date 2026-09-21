//! The representation, as text a person reads.
//!
//! # Why this exists at all
//!
//! Nothing in this workspace could print this IR. The only IR dump that existed
//! was the old engine's, over a different representation, so every question
//! about what this engine emits — did the widening fold, is that guard still
//! there, how many blocks did the cleanup copy become — had to be answered by
//! reading the emitter instead of its output. An optimizer that cannot be read
//! is one that gets changed by argument.
//!
//! # What was rejected
//!
//! A match arm per instruction, spelling each one in an operator syntax. That is
//! the readable form, and it is also a second statement of the vocabulary: an
//! instruction added to [`super::inst::Inst`] would compile fine here and print
//! as nothing, which is the drift `#[deny(dead_code)]` and the verifier exist to
//! prevent elsewhere. So an instruction prints through its own derived `Debug`,
//! and the readability is bought where it is cheap instead — [`super::entity`]
//! spells its handles `v0`, `block2`, `c1`, so the derived output already reads
//! as operands rather than as constructors.
//!
//! What this file does own is the SHAPE: signature, block order, parameters,
//! the values an instruction defines, and the terminator. Those are structural
//! and there is a fixed number of them.
//!
//! # Determinism
//!
//! Rule 13. Everything printed is walked in table order — blocks in creation
//! order, instructions in block order, constants by handle. No iteration over a
//! map, so two builds of the same program produce the same text and a diff of
//! two dumps is a diff of two programs.

use core::fmt;

use super::entity::ConstId;
use super::func::Function;

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.header())?;
        for (id, decl) in self.constants() {
            writeln!(f, "    {id:?} = {decl:?}")?;
        }
        for (id, block) in self.blocks() {
            let params: Vec<String> = block
                .params
                .iter()
                .map(|&v| format!("{v:?}: {:?}", self.repr_of(v)))
                .collect();
            match params.is_empty() {
                true => writeln!(f, "{id:?}:")?,
                false => writeln!(f, "{id:?}({}):", params.join(", "))?,
            }
            if let Some(region) = self.region_of(id) {
                writeln!(f, "    ; protected by {region:?}")?;
            }
            for &inst_id in &block.insts {
                let data = self
                    .inst(inst_id)
                    .expect("a block lists only instructions of its own function");
                let defines: Vec<String> = data
                    .results
                    .iter()
                    .map(|&v| format!("{v:?}: {:?}", self.repr_of(v)))
                    .collect();
                let position = self.position_of(inst_id);
                let origin = match position.is_known() {
                    true => format!("    ; at {}", position.raw()),
                    false => String::new(),
                };
                match defines.is_empty() {
                    true => writeln!(f, "    {:?}{origin}", data.inst)?,
                    false => writeln!(
                        f,
                        "    {} = {:?}{origin}",
                        defines.join(", "),
                        data.inst
                    )?,
                }
            }
            // An absent terminator is printed rather than skipped. A block with
            // no exit is what a half-built function looks like, and this dump is
            // most useful exactly when something is half-built — the verifier
            // will refuse it later, which is too late to be reading the dump.
            match &block.terminator {
                Some(terminator) => writeln!(f, "    {terminator:?}")?,
                None => writeln!(f, "    <no terminator>")?,
            }
        }
        Ok(())
    }
}

impl Function {
    /// The one-line summary: what it accepts, what it returns, and the two
    /// properties of it a caller cannot re-decide.
    fn header(&self) -> String {
        let signature = &self.signature;
        let params: Vec<String> = signature.params.iter().map(|r| format!("{r:?}")).collect();
        let returns: Vec<String> = signature.returns.iter().map(|r| format!("{r:?}")).collect();
        let suspends = match signature.may_suspend {
            true => ", may suspend",
            false => "",
        };
        format!(
            "function({}) -> ({}) [{:?}{suspends}], entry {:?}",
            params.join(", "),
            returns.join(", "),
            signature.convention,
            self.entry,
        )
    }

    /// Every constant this function declared, by handle.
    ///
    /// Public because the dump is not the only reason to walk them — an
    /// optimization that folds a constant needs the same list — and private
    /// would have meant the renderer reaching into the table directly, which is
    /// what `constant()` exists to prevent.
    pub fn constants(&self) -> impl Iterator<Item = (ConstId, &super::consts::ConstDecl)> {
        (0..).map(ConstId).map_while(|id| Some((id, self.constant(id)?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::Convention;
    use crate::ir::{Inst, NumOp, Signature, Terminator};
    use crate::repr::Repr;

    fn adding_two_parameters() -> Function {
        let mut func = Function::new(Signature {
            params: vec![Repr::I64, Repr::I64],
            returns: vec![Repr::I64],
            may_suspend: false,
            convention: Convention::default(),
        });
        let entry = func.entry;
        let (a, b) = {
            let block = func.block(entry).expect("the entry block");
            (block.params[0], block.params[1])
        };
        let sum = func.push_inst(entry, Inst::IntArith(NumOp::Add, a, b), &[Repr::I64])[0];
        func.set_terminator(entry, Terminator::Return(vec![sum]));
        func
    }

    #[test]
    fn a_dump_names_operands_as_values_not_as_constructors() {
        let text = adding_two_parameters().to_string();
        assert!(
            text.contains("v2: I64 = IntArith(Add, v0, v1)"),
            "an instruction should read as its operands; got:\n{text}"
        );
    }

    #[test]
    fn a_dump_states_the_signature_and_the_entry_block() {
        let text = adding_two_parameters().to_string();
        assert!(
            text.starts_with("function(I64, I64) -> (I64)"),
            "the header should say what the function accepts and returns; got:\n{text}"
        );
        assert!(
            text.contains("block0(v0: I64, v1: I64):"),
            "a block should name the parameters it binds; got:\n{text}"
        );
    }

    #[test]
    fn two_dumps_of_one_function_are_the_same_text() {
        // Rule 13: a diff that changes for no reason is a diff nobody reads.
        let first = adding_two_parameters().to_string();
        let second = adding_two_parameters().to_string();
        assert_eq!(first, second, "the dump must not depend on iteration order");
    }
}
