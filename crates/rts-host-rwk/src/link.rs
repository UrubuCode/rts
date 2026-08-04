//! The agreements between the three crates, made explicit.
//!
//! Two facts have to line up before a compiled program is correct, and neither
//! is checked by a type. This module is where they are stated, because this is
//! the first place either can be observed at all.

use rts_codegen::emit::EmitError;
use rts_codegen::values::{Singleton, ValueModel};
use rts_core_rwk::value::Singletons;
use rts_cranelift::target::TargetError;

/// Why a program could not be hosted.
#[derive(Debug)]
pub enum HostError {
    /// The source did not parse.
    Parse(String),
    /// The language could not emit it.
    Emit(EmitError),
    /// The machine could not place it.
    Place(TargetError),
    /// The emitted program is not well formed.
    ///
    /// A defect in `rts-codegen`, never in the program being compiled. It is a
    /// distinct case rather than folded into [`Self::Place`] because the two
    /// call for opposite responses: a placement failure is about this machine,
    /// and this one is about the compiler.
    Malformed(String),
    /// The program calls a runtime operation this host cannot supply.
    ///
    /// The failure the two independent statements of the entry-point set were
    /// always going to produce. `rts-codegen` names what it emits calls to and
    /// `rts-core-rwk` exports what it defines; nothing ties them together at
    /// compile time, and this is where the disagreement becomes visible instead
    /// of becoming a call to the wrong address.
    NoSuchRuntimeOperation {
        /// The symbol the compiled program expects.
        symbol: &'static str,
    },
}

impl From<EmitError> for HostError {
    fn from(error: EmitError) -> Self {
        HostError::Emit(error)
    }
}

impl From<TargetError> for HostError {
    fn from(error: TargetError) -> Self {
        HostError::Place(error)
    }
}

/// The runtime's singleton numbering, taken from the compiler's.
///
/// # Why this is not two constants
///
/// `undefined` and `null` are numbered by the machine's tag registry when the
/// language declares them, and the runtime holds its own record of which number
/// means what. Nothing makes those agree: the compiler could emit singleton 0
/// for `undefined` while the runtime read 0 as `null`, and every program would
/// still compile.
///
/// The failure would be quiet and total — `x === null` answering for the wrong
/// value — so the numbering is copied from one side to the other here rather
/// than written down twice.
pub fn singletons_for(model: &ValueModel) -> Singletons {
    Singletons {
        undefined: model.singleton(Singleton::Undefined).number(),
        null: model.singleton(Singleton::Null).number(),
    }
}
