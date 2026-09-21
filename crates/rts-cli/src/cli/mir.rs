//! `rts mir` — the shared IR of a program, one stage before `rts ir`.
//!
//! # Why a command of its own and not a flag on `rts ir`
//!
//! Because the two answer about different stages, and the difference is the reason
//! this stage exists. `rts ir` shows the machine's representation: by then a type
//! domain's proof has become an offset and a guard has become a branch, so "what
//! did the analysis know here" has no answer left to read. `rts mir` shows the
//! graph while it still does.
//!
//! It also shows what does NOT lower yet. The MIR stage is being built underneath a
//! working compiler, so its coverage is partial by construction, and the refusals
//! are printed beside the graphs: what a real program is refused for, counted, is
//! which lowering to write next. A dump that showed only what worked would report
//! coverage the stage does not have.
//!
//! Printed to stdout for the reason `rts ir` records: the previous form of that
//! command went to stderr, so redirecting its output wrote an empty file.

use std::path::PathBuf;

use anyhow::{Result, anyhow};

pub fn command(input: Option<String>, specialised: bool, machine: bool) -> Result<()> {
    let input = input
        .ok_or_else(|| anyhow!("usage: rts mir [--specialised] <input.ts | inline-source>"))?;
    let path = PathBuf::from(&input);
    let source = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|held| anyhow!("unreadable: {} ({held})", path.display()))?
    } else if input.ends_with(".ts") || input.ends_with(".js") {
        return Err(anyhow!("input file not found: {}", path.display()));
    } else {
        // Not a file and not named like one: an inline snippet, as `rts ir` and
        // `eval` both accept.
        input.clone()
    };
    // THE SPECIALISED TIER ON REQUEST, because it is the one a guard exists in and the
    // command could not show it. `rts mir --specialised file.ts`.
    // `--machine` asks the OTHER question: not what the graph looks like but how much
    // of it becomes code. A graph is refused at the machine boundary for things the
    // graph cannot show, so reading the lowering share as the machine share would
    // overstate the stage by a long way.
    if machine {
        let text = rts_host::describe::describe_mir_machine(&source)
            .map_err(|held| anyhow!("{held:?}"))?;
        print!("{text}");
        return Ok(());
    }
    let text = match specialised {
        true => rts_host::describe::describe_mir_specialised(&source),
        false => rts_host::describe::describe_mir(&source),
    }
    .map_err(|held| anyhow!("{held:?}"))?;
    print!("{text}");
    Ok(())
}
