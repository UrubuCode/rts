//! Per-target rules, as data.
//!
//! The classifier is one algorithm. What differs between targets is a handful of
//! numbers and one policy, so targets are described rather than implemented —
//! duplicating the algorithm per target is how the rules drift apart.

/// How a target passes an aggregate by value.
///
/// The three policies here are genuinely different rules, not three tunings of
/// one rule, which is why this is an enumeration rather than more numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AggregatePolicy {
    /// Split into register-sized pieces, each classified by what it contains.
    ///
    /// A piece holding only floating-point fields travels in a floating-point
    /// register; anything else travels in an integer register.
    Pieces {
        /// Largest aggregate that may travel in registers at all.
        max_bytes: u32,
        /// Size of one piece.
        piece_bytes: u32,
    },

    /// One register, and only for a size that is a power of two.
    ///
    /// Anything else travels by reference. There is no two-register case.
    SingleRegister {
        /// Largest aggregate that may travel in a register.
        max_bytes: u32,
    },

    /// Registers up to a size, with a separate allowance for an aggregate whose
    /// fields are all the same floating-point type.
    Homogeneous {
        /// Largest aggregate that may travel in registers.
        max_bytes: u32,
        /// Most members a homogeneous floating-point aggregate may have.
        max_float_members: u32,
    },
}

/// What one target's boundary looks like.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TargetAbi {
    /// For diagnostics only. Never branched on.
    pub name: &'static str,
    /// Integer registers available to parameters.
    pub int_param_registers: u8,
    /// Floating-point registers available to parameters.
    pub float_param_registers: u8,
    /// Integer registers available to returns.
    pub int_return_registers: u8,
    /// Floating-point registers available to returns.
    pub float_return_registers: u8,
    /// How an aggregate travels by value.
    pub aggregate: AggregatePolicy,
    /// How many returns may travel in registers.
    ///
    /// Deliberately conservative. The signature type permits any number and the
    /// register budget is known, but whether more than one return is legalized
    /// end to end on a given target has not been exercised here. Until a fixture
    /// proves a count on a target, this stays at one and everything else uses
    /// the out-pointer form, which is unambiguously supported. Raise it with
    /// [`TargetAbi::with_verified_direct_returns`] once proven — and only then.
    pub max_direct_returns: u8,
}

impl TargetAbi {
    /// The stable convention on 64-bit x86 outside Windows.
    pub fn x86_64_sysv() -> Self {
        Self {
            name: "x86_64-sysv",
            int_param_registers: 6,
            float_param_registers: 8,
            int_return_registers: 2,
            float_return_registers: 2,
            aggregate: AggregatePolicy::Pieces {
                max_bytes: 16,
                piece_bytes: 8,
            },
            max_direct_returns: 1,
        }
    }

    /// The stable convention on 64-bit x86 under Windows.
    ///
    /// Notably stingier: four register parameters regardless of type, and an
    /// aggregate larger than one register always travels by reference.
    pub fn x86_64_windows() -> Self {
        Self {
            name: "x86_64-windows",
            int_param_registers: 4,
            float_param_registers: 4,
            int_return_registers: 1,
            float_return_registers: 1,
            aggregate: AggregatePolicy::SingleRegister { max_bytes: 8 },
            max_direct_returns: 1,
        }
    }

    /// The stable convention on 64-bit ARM.
    pub fn aarch64() -> Self {
        Self {
            name: "aarch64",
            int_param_registers: 8,
            float_param_registers: 8,
            int_return_registers: 2,
            float_return_registers: 4,
            aggregate: AggregatePolicy::Homogeneous {
                max_bytes: 16,
                max_float_members: 4,
            },
            max_direct_returns: 1,
        }
    }

    /// Raises how many returns may travel in registers.
    ///
    /// Call this only with a count a fixture has actually demonstrated on this
    /// target. The point of the low default is that an unproven count fails
    /// safely, as an extra indirection, rather than unsafely, as a wrong ABI.
    pub fn with_verified_direct_returns(mut self, count: u8) -> Self {
        self.max_direct_returns = count;
        self
    }

    /// Largest aggregate this target will pass in registers.
    pub fn aggregate_register_limit(&self) -> u32 {
        match self.aggregate {
            AggregatePolicy::Pieces { max_bytes, .. }
            | AggregatePolicy::SingleRegister { max_bytes }
            | AggregatePolicy::Homogeneous { max_bytes, .. } => max_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_starts_conservative_about_multiple_returns() {
        for target in [
            TargetAbi::x86_64_sysv(),
            TargetAbi::x86_64_windows(),
            TargetAbi::aarch64(),
        ] {
            assert_eq!(
                target.max_direct_returns, 1,
                "{}: an unproven count must fail as an indirection, not as a wrong ABI",
                target.name
            );
        }
    }

    #[test]
    fn raising_the_count_is_explicit() {
        let target = TargetAbi::x86_64_sysv().with_verified_direct_returns(2);
        assert_eq!(target.max_direct_returns, 2);
    }
}
