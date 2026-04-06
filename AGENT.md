# RTS Agent Guide

## Objective

RTS is a compiler/runtime bootstrap where:

- Rust provides only **raw, low-level runtime APIs**.
- TypeScript packages in `packages/*` build all **high-level behavior**.

This file defines project boundaries for contributors and agents.

## Architecture

1. Rust core (`src/`)
- Compiler pipeline, module resolver, diagnostics, bundle format.
- Runtime primitive exports under module `"rts"` (raw operations only).

2. TypeScript packages (`packages/*`)
- Implement ergonomic APIs (`console`, `process`, `std`, `fs`, etc.).
- Compose raw `"rts"` primitives into developer-facing modules.

3. Types
- `packages/rts-types/rts.d.ts` contains only declarations for module `"rts"`.
- Each package owns its own typings:
  - example: `packages/process/process.d.ts` declares module `"process"`.

## Hard Rules

1. Do not implement package APIs inside Rust interpreter/bootstrap logic.
- No special-case semantics for `"console"`, `"process"`, or other high-level modules in Rust.

2. Do not place `packages/**/*` declarations inside `rts.d.ts`.
- `rts.d.ts` is only for the raw runtime contract (`declare module "rts"`).

3. Keep Rust runtime surface raw and minimal.
- High-level sugar, formatting, and module behaviors belong to TypeScript packages.

## Practical Guidance

When adding a new capability:

1. Expose primitive in Rust module `"rts"` if needed.
2. Wrap it in a package under `packages/<name>/main.ts`.
3. Add package-owned typing file (e.g. `packages/<name>/<name>.d.ts`).
4. Keep examples importing from packages, not from Rust internals.

## Verification Checklist

- `cargo run -- build examples/console.ts target/console`
- `cargo run -- examples/console.ts`
- Ensure no new `declare module "<package>"` appears in `packages/rts-types/rts.d.ts`.
