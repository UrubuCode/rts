//! Which part of the program a run of emitted bytes came from.

use cranelift_codegen::CompiledCode;

use crate::fault::Position;

/// Where each run of emitted code came from, for one function.
///
/// Ranges rather than points: one instruction becomes several machine
/// instructions, and several of ours can collapse into one. Recording a point
/// per emitted byte would be both larger and less true than recording the run it
/// belongs to.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PositionMap {
    runs: Vec<Run>,
}

/// One run of emitted code, and where it came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Run {
    start: u32,
    end: u32,
    position: Position,
}

impl PositionMap {
    /// Reads the map out of a compiled function.
    ///
    /// Everything here comes from what the code generator recorded while
    /// emitting, which is filled in because lowering said where each instruction
    /// came from — and is empty otherwise, which is a truthful empty rather than
    /// a silent one.
    pub fn of(code: &CompiledCode) -> Self {
        let runs = code
            .buffer
            .get_srclocs_sorted()
            .iter()
            .filter(|range| range.start < range.end)
            .map(|range| Run {
                start: range.start,
                end: range.end,
                position: Position::from_recorded(range.loc.bits()),
            })
            .collect();
        Self { runs }
    }

    /// Where the code at an offset came from, if anything said.
    ///
    /// Answered by bisection: the runs do not overlap and are in order, so the
    /// one that could contain an offset is the last one starting at or before it.
    pub fn at(&self, offset: u32) -> Option<Position> {
        let index = match self.runs.binary_search_by_key(&offset, |run| run.start) {
            Ok(exact) => exact,
            Err(0) => return None,
            Err(after) => after - 1,
        };
        let run = self.runs[index];
        (offset < run.end && run.position.is_known()).then_some(run.position)
    }

    /// How many distinct runs the function was emitted as.
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    /// Whether nothing said where any of it came from.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Every position this function was built from, in address order.
    ///
    /// Deduplicated, because one place in a program becoming several runs of
    /// code is ordinary and a reader asking "what is in here" does not want it
    /// listed once per run.
    pub fn positions(&self) -> Vec<Position> {
        let mut seen: Vec<Position> = Vec::new();
        for run in &self.runs {
            if run.position.is_known() && !seen.contains(&run.position) {
                seen.push(run.position);
            }
        }
        seen
    }
}
