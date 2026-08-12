//! Counting what the claims would be worth, before spending a week finding out.
//!
//! # Why the first phase of a type pass is a counter
//!
//! Because this repository has killed four optimisation campaigns by measuring
//! them, and one of them was killed by exactly this mechanism for exactly this
//! price: `RTS_ESCAPE_STATS` reported 299 candidate objects and 10 actual
//! replacements — 3.3% — and the direction that number closed had looked
//! obviously worth doing.
//!
//! A type pass has the same shape of risk. "Annotate the hot function and it
//! gets fast" is a thesis everybody believes before they look, and the looking
//! is one environment variable.
//!
//! # The counter that is expected to be zero, and why it is here
//!
//! `operator-sites-claimed-number` counts binary-operator sites where an operand
//! carries a surviving `number` claim. **It is expected to be pure loss.**
//! `emit::expr::emit_binary` already speculates unconditionally — an operand
//! pair nothing proved already gets two guards and a slow path — so a claim at
//! one of those sites asks for a guard that is already emitted.
//!
//! It is counted anyway, and prominently, because the next person to arrive with
//! the obvious idea should meet a number rather than an argument.
//!
//! # Not a static
//!
//! The counters live on `Ctx` and print from the outermost emission. A process
//! global would be the shorter code and the wrong one: cargo runs a binary's
//! tests in threads, and this workspace has already paid for shared statics in
//! test binaries once.

use super::{Facts, Kind};

/// What one compilation's annotations amounted to.
#[derive(Default)]
pub(in crate::emit) struct Census {
    /// Claims that named one kind of value, by what carried them.
    pub(in crate::emit) definite_parameter: usize,
    pub(in crate::emit) definite_binding: usize,
    /// Claims that survived the body — not contradicted, not already proved.
    pub(in crate::emit) live: usize,
    /// Of those, the ones the machine has an exact guard for.
    pub(in crate::emit) spendable_number: usize,
    pub(in crate::emit) spendable_boolean: usize,
    /// Of those, the ones it does not: five of the nine `Claim` variants.
    pub(in crate::emit) unspendable_str: usize,
    pub(in crate::emit) unspendable_array: usize,
    pub(in crate::emit) unspendable_object: usize,
    pub(in crate::emit) unspendable_other: usize,
    /// Claims dropped because the body had already proved the same name.
    ///
    /// The campaign buys nothing on these: `proven` got there first, and its
    /// answer is a proof rather than a guess.
    pub(in crate::emit) redundant_already_proved: usize,
    /// Binary-operator sites with a claimed-`number` operand.
    ///
    /// The one expected to be zero-value. See this module's opening note.
    pub(in crate::emit) operator_sites_claimed_number: usize,
    /// Binary-operator sites with an operand claimed something that is NOT a
    /// number — where a later phase could refuse a speculation that will fail.
    pub(in crate::emit) operator_sites_claimed_non_number: usize,
    /// How many function bodies were looked at, so a rate can be computed.
    pub(in crate::emit) bodies: usize,
}

impl Census {
    /// Folds one body's answer in.
    ///
    /// `dropped` is how many claims the body contradicted or had already proved,
    /// which `analyse` knows and `Facts` no longer does — a fact that survives
    /// only in the difference has to be handed over rather than recovered.
    pub(in crate::emit) fn record(&mut self, facts: &Facts, parameters: usize, redundant: usize) {
        self.bodies += 1;
        self.definite_parameter += parameters;
        self.redundant_already_proved += redundant;
        self.live += facts.len();
        self.definite_binding += facts.len().saturating_sub(parameters);
        for (_, kind) in facts.kinds() {
            // Spendable is ASKED rather than listed: `Kind::repr` is the one
            // statement of which claims the machine has an exact guard for, and
            // a second list here would be a second answer that drifts the day a
            // guard is added.
            match kind.repr() {
                Some(rts_cranelift::repr::Repr::F64) => self.spendable_number += 1,
                Some(_) => self.spendable_boolean += 1,
                None => match kind {
                    Kind::Str => self.unspendable_str += 1,
                    Kind::Array => self.unspendable_array += 1,
                    Kind::Instance(_) => self.unspendable_object += 1,
                    Kind::Other => self.unspendable_other += 1,
                    Kind::Number | Kind::Boolean | Kind::Unknown => {}
                },
            }
        }
    }

    /// One binary-operator site whose operand carries a claim.
    ///
    /// Split by whether the claim is `number`, because the two answer different
    /// questions. A claimed number is the counter expected to be worth NOTHING —
    /// `emit_binary` already speculates there. A claimed non-number is the
    /// opposite: a site where speculation is known to be about to fail, and
    /// refusing it is a real saving a later phase can take.
    pub(in crate::emit) fn operand_claimed(&mut self, kind: Kind) {
        match kind {
            Kind::Number => self.operator_sites_claimed_number += 1,
            _ => self.operator_sites_claimed_non_number += 1,
        }
    }

    /// Whether anything asked for this.
    ///
    /// Read once per compilation rather than per site: an environment lookup on
    /// a path that runs per operation is a cost this repository has already
    /// found and named elsewhere.
    pub(in crate::emit) fn wanted() -> bool {
        std::env::var_os("RTS_CLAIM_STATS").is_some()
    }

    /// Prints what was counted, one `claim-census` line per counter.
    ///
    /// The format follows `RTS_ESCAPE_STATS` and `RTS_TIMING` so that a reader
    /// who has seen one has seen all three, and so a shell can sum a column.
    pub(in crate::emit) fn report(&self) {
        let spendable = self.spendable_number + self.spendable_boolean;
        let unspendable = self.unspendable_str
            + self.unspendable_array
            + self.unspendable_object
            + self.unspendable_other;
        for (name, value) in [
            ("bodies", self.bodies),
            ("definite-parameter", self.definite_parameter),
            ("definite-binding", self.definite_binding),
            ("live", self.live),
            ("redundant-already-proved", self.redundant_already_proved),
            ("spendable-number", self.spendable_number),
            ("spendable-boolean", self.spendable_boolean),
            ("spendable-total", spendable),
            ("unspendable-str", self.unspendable_str),
            ("unspendable-array", self.unspendable_array),
            ("unspendable-object", self.unspendable_object),
            ("unspendable-other", self.unspendable_other),
            ("unspendable-total", unspendable),
            (
                "operator-sites-claimed-number",
                self.operator_sites_claimed_number,
            ),
            (
                "operator-sites-claimed-non-number",
                self.operator_sites_claimed_non_number,
            ),
        ] {
            eprintln!("claim-census {name} {value}");
        }
    }
}
