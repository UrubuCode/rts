//! `IsCompatiblePropertyDescriptor` — the definition check, asked about a
//! property that is described rather than stored.
//!
//! # Why this is not a second copy of the check
//!
//! A proxy's `getOwnPropertyDescriptor` and `defineProperty` traps (ES2025
//! §10.5.5 step 14, §10.5.6 step 16.a) must refuse an answer that
//! `ValidateAndApplyPropertyDescriptor` would refuse against the target's own
//! property — the same comparison a definition makes, with nothing written.
//! [`super::descriptor::permitted`] IS that comparison, so this only builds the
//! record it reads out of what the caller holds: a target that is itself a
//! proxy has no cell to read, only the descriptor its own trap answered.
//! Writing the rules again here would be two answers to which redefinitions a
//! non-configurable property allows.

use super::descriptor::{Current, Descriptor, permitted};
use crate::entry::integrity::Attributes;
use crate::entry::with_current;

/// A property as the target reports it: a data property holding `value`, or —
/// when `value` is `None` — an accessor with the halves it has.
pub(in crate::entry) struct Existing {
    /// What a data property holds; `None` marks an accessor.
    pub(in crate::entry) value: Option<u64>,
    /// An accessor's getter, `None` when it has none.
    pub(in crate::entry) get: Option<u64>,
    /// An accessor's setter, `None` when it has none.
    pub(in crate::entry) set: Option<u64>,
    /// The three flags.
    pub(in crate::entry) attributes: Attributes,
}

/// Whether defining `wanted` over `current` on an object whose extensibility is
/// `extensible` would be accepted.
pub(in crate::entry) fn compatible(
    extensible: bool,
    wanted: &Descriptor,
    current: Option<&Existing>,
) -> bool {
    let current = current.map(|held| match held.value {
        Some(value) => Current::Data {
            value,
            attributes: held.attributes,
        },
        None => Current::Accessor {
            get: held.get,
            set: held.set,
            attributes: held.attributes,
        },
    });
    with_current(|context| permitted(context, current.as_ref(), wanted, extensible))
}
