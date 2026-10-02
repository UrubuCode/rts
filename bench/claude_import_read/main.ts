// What a LIVE named import costs at the point of use.
//
// A named import used to be one `ModuleBinding` call at the import line, stored
// in a local, so every later use was a register read. It is now the exporting
// module's namespace bound once and a property read of it per use — which is the
// cost this file exists to put a number on, since `emit/module.rs` chose it over
// a runtime crossing per use.
//
// The loop reads two imported constants and calls one imported function on every
// pass, which is the worst shape for the change: nothing between the reads, so
// the property read is the whole of the body's cost rather than a line in it.
// Real code reads an import once and works with it.
//
// # Why this benchmark is rts-only, where `bench/analytic.ts` is not
//
// Because it is ABOUT imports, so it cannot be one file — and `node` will not
// import a relative `.ts`. The convention `bench/analytic.ts` states in its own
// header (runs unmodified under rts, node and bun, therefore no imports) is the
// convention this measurement cannot have and still measure anything.
//
// Run it in RELEASE, never `--profile fast`:
//
//     cargo build --release
//     target/release/rts.exe run bench/claude_import_read/main.ts
//
// And compare against a kept baseline binary, per file, as CLAUDE.md requires of
// any number.

import { STEP, LIMIT, mix } from "./lib";

function pass(): number {
    let total = 0;
    for (let i = 0; i < LIMIT; i = i + STEP) {
        total = mix(total, i);
    }
    return total;
}

// Warm, then measure the minimum of several runs: the minimum is the one that is
// not measuring the machine being busy.
let best = 1e18;
let answer = 0;
for (let round = 0; round < 40; round = round + 1) {
    const started = Date.now();
    for (let repeat = 0; repeat < 2000; repeat = repeat + 1) {
        answer = pass();
    }
    const took = Date.now() - started;
    if (took < best) {
        best = took;
    }
}

const passes = 2000 * Math.ceil(LIMIT / STEP);
console.log("answer", answer);
console.log("best ms for 2000 passes", best);
console.log("ns per loop iteration", (best * 1e6) / passes);
