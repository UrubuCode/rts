//! What can fault in one compiled function, and where each came from.

use cranelift_codegen::CompiledCode;

use super::Position;

/// Why a program stopped, as the machine reports it.
///
/// Kept apart from the reasons a client asked for. A client's trap and a
/// division by zero both stop the program, but a reader chasing one wants to
/// know immediately which kind it was, and a single opaque number would make
/// that a lookup.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FaultKind {
    /// A point the program said was unreachable.
    Unreachable,
    /// An index outside its bounds.
    OutOfBounds,
    /// An integer division by zero.
    DivideByZero,
    /// Something else the machine traps on.
    ///
    /// Carries the number so that a reader is not left guessing, without this
    /// enumeration having to grow every time a target adds a reason.
    Other(u8),
}

/// One place a compiled function can stop.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fault {
    /// Where in the emitted code, from the start of the function.
    pub offset: u32,
    /// Why it stops.
    pub kind: FaultKind,
    /// Where in the client's program it came from, if anywhere.
    pub position: Position,
}

/// Every place one compiled function can stop.
///
/// Ordered by code offset, because the question asked of it is always "something
/// stopped at this address" — which arrives from a stack walk, one return
/// address at a time, and is answered by bisection rather than by scanning.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FaultTable {
    faults: Vec<Fault>,
}

impl FaultTable {
    /// Reads the faults out of a compiled function.
    ///
    /// The positions come from the code generator's own record of which range of
    /// emitted bytes belongs to which source location — which is filled in
    /// because lowering said so at every instruction, and would be empty
    /// otherwise.
    pub fn of(code: &CompiledCode) -> Self {
        let srclocs = code.buffer.get_srclocs_sorted();

        let mut faults: Vec<_> = code
            .buffer
            .traps()
            .iter()
            .map(|trap| Fault {
                offset: trap.offset,
                kind: kind_of(trap.code),
                position: srclocs
                    .iter()
                    .find(|range| range.start <= trap.offset && trap.offset < range.end)
                    .map(|range| Position(range.loc.bits()))
                    .unwrap_or(Position::UNKNOWN),
            })
            .collect();

        faults.sort_by_key(|fault| fault.offset);
        Self { faults }
    }

    /// What stops at an address, if anything does.
    pub fn at(&self, offset: u32) -> Option<Fault> {
        self.faults
            .binary_search_by_key(&offset, |fault| fault.offset)
            .ok()
            .map(|index| self.faults[index])
    }

    /// Every fault, in address order.
    pub fn iter(&self) -> impl Iterator<Item = Fault> + '_ {
        self.faults.iter().copied()
    }

    /// How many places this function can stop.
    pub fn len(&self) -> usize {
        self.faults.len()
    }

    /// Whether it can stop anywhere.
    pub fn is_empty(&self) -> bool {
        self.faults.is_empty()
    }
}

/// Translates the code generator's reason into ours.
///
/// The three the representation names are matched by name; anything else keeps
/// its number rather than being flattened into one bucket, because a reader
/// looking at a fault wants to know what it was even when we have no name for it.
fn kind_of(code: cranelift_codegen::ir::TrapCode) -> FaultKind {
    use cranelift_codegen::ir::TrapCode as Cl;
    match code {
        Cl::HEAP_OUT_OF_BOUNDS => FaultKind::OutOfBounds,
        Cl::INTEGER_DIVISION_BY_ZERO => FaultKind::DivideByZero,
        other if other == Cl::unwrap_user(1) => FaultKind::Unreachable,
        other => FaultKind::Other(other.as_raw().get()),
    }
}
