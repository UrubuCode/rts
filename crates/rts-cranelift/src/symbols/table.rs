//! Declaring entry points into a module, and recording which ones were used.
//!
//! Keyed by the discriminant, which is a dense index, so a lookup is an array
//! read. The alternative — declaring by name wherever one is called — allocates
//! a string and probes a hash table per call site, which makes the cost
//! proportional to how often something is called rather than to how many things
//! there are.
//!
//! # Why all of them are declared before any function is lowered
//!
//! Lowering used to declare an entry point on first use, which meant lowering
//! handed out a module identifier — and an identifier handed out while a worker
//! runs is an identifier whose number depends on which worker got there first.
//! Rule 13 (`README.md`) forbids exactly that, so this was the last thing keeping
//! lowering on one thread.
//!
//! It hoists outright because [`RtEntry::ALL`] is a **closed set of seven** whose
//! signatures are a property of this crate rather than of any program: nothing
//! about a program can change what is declared, only whether it is reached. So
//! the seven are declared once, in numbering order, before anything is lowered,
//! and [`EntryImports`] is read-only from then on.
//!
//! What that costs is stated where it is paid: see [`EntryTable`].

use cranelift_module::{FuncId as MachineFuncId, Linkage, Module, ModuleError};

use super::RtEntry;
use crate::lower::machine_signature;

/// The module identifier of each entry point, for one module.
///
/// Bound to that module, like every other identifier the module issues. A fresh
/// module needs a fresh set; reusing one is how a call comes to name something in
/// a module that is no longer there.
pub struct EntryImports {
    declared: [Option<MachineFuncId>; RtEntry::COUNT],
}

impl EntryImports {
    /// Declares every entry point into a module, in numbering order.
    ///
    /// Order is stated rather than incidental: it is the order two builds must
    /// agree on for the module's own identifiers to line up, and numbering order
    /// is the one order that is a fact about this crate instead of about a
    /// program.
    pub fn declare_all(module: &mut dyn Module) -> Result<Self, ModuleError> {
        let call_conv = module.isa().default_call_conv();
        let mut declared = [None; RtEntry::COUNT];
        for &entry in RtEntry::ALL {
            let signature = machine_signature(&entry.signature(), call_conv);
            // Imported: the runtime provides it. On one path a linker resolves
            // that against an archive; on the other an address was registered
            // before anything was compiled. Neither is this module's business.
            declared[entry.index()] =
                Some(module.declare_function(entry.symbol(), Linkage::Import, &signature)?);
        }
        Ok(Self { declared })
    }

    /// The module's identifier for an entry point.
    ///
    /// Infallible because [`declare_all`](Self::declare_all) is the only way to
    /// obtain one of these and it fills every slot. A fallible accessor would be
    /// an error path no program can reach, which rule 6 calls dead code.
    pub fn machine_id(&self, entry: RtEntry) -> MachineFuncId {
        self.declared[entry.index()].expect("every entry point is declared up front")
    }
}

/// Which entry points a compilation's code actually names.
///
/// # Why this is separate from [`EntryImports`]
///
/// Because declaring all seven up front makes "declared" stop meaning "used",
/// and "used" is the question worth being able to ask: it is what says a
/// compiled program reaches for the allocator, and — for the object-file path —
/// which of the seven undefined symbols an artefact would still need if the
/// unreached ones were pruned out of it.
///
/// Declaring lazily was the older answer to the same question and it cost
/// determinism, which is not a trade this crate makes: a size optimisation is
/// worth less than compiling one program twice into the same bytes. Recorded
/// after lowering instead, serially, so nothing about it depends on which worker
/// finished first.
#[derive(Clone)]
pub struct EntryTable {
    referenced: [bool; RtEntry::COUNT],
}

impl Default for EntryTable {
    fn default() -> Self {
        Self {
            referenced: [false; RtEntry::COUNT],
        }
    }
}

impl EntryTable {
    /// A compilation that has named nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that a function named an entry point.
    pub(crate) fn mark(&mut self, entry: RtEntry) {
        self.referenced[entry.index()] = true;
    }

    /// Adds everything another compilation named to this one.
    pub(crate) fn union(&mut self, other: &EntryTable) {
        for (ours, theirs) in self.referenced.iter_mut().zip(other.referenced) {
            *ours |= theirs;
        }
    }

    /// Whether an entry point is one this compilation's code names.
    ///
    /// Which is also the answer to whether its import declaration is one the
    /// artefact needs — see the type's own documentation for why the two used to
    /// be the same question and no longer are.
    pub fn is_declared(&self, entry: RtEntry) -> bool {
        self.referenced[entry.index()]
    }

    /// Every entry point this compilation's code names, in numbering order.
    ///
    /// The list a pruner would keep. Ordered by the numbering rather than by
    /// anything observed while compiling, so two builds produce the same list.
    pub fn reached(&self) -> impl Iterator<Item = RtEntry> + '_ {
        RtEntry::ALL
            .iter()
            .copied()
            .filter(|entry| self.is_declared(*entry))
    }

    /// How many entry points this compilation's code names.
    pub fn len(&self) -> usize {
        self.referenced.iter().filter(|used| **used).count()
    }

    /// Whether it names none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
