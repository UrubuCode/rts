//! Which closures this stage makes are born light.
//!
//! `emit/light_call.rs` is which functions are light and why; `rts-core`'s
//! `entry/light_call.rs` is what the runtime does with the answer, and the two
//! designs that made the CALL SITE different and lost. Here the call site is
//! the door it always was: what differs is the operation that makes the
//! closure, chosen by asking whether the code address it is made over came from
//! the constant of a function in the set.

use rts_cranelift::ir::ValueId as MachineValue;

use super::JsMachine;

impl<'a> JsMachine<'a> {
    /// Tells this boundary which functions the compiler found light.
    pub fn light_functions(
        mut self,
        held: &'a std::collections::BTreeSet<rts_cranelift::ir::FuncId>,
    ) -> Self {
        self.light = Some(held);
        self
    }
}

impl JsMachine<'_> {
    /// Whether `code` is the address of a function found light: a machine value
    /// the declared constant of a function produced, whose id is in the set.
    pub(super) fn is_light_code(&self, code: MachineValue) -> bool {
        let Some(light) = self.light else {
            return false;
        };
        let Some(index) = self.from_constant.get(&code) else {
            return false;
        };
        let Some(crate::domain::JsConst::Function(which)) = self.domain.declared(*index) else {
            return false;
        };
        self.shared
            .as_ref()
            .and_then(|shared| shared.module.get(*which as usize))
            .is_some_and(|id| light.contains(id))
    }
}
