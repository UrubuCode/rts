<div align="center">

<img src=".github/imgs/logo.png" alt="RTS logo" width="220" />

# RTS

### TypeScript and JavaScript compiled to native code.

A Rust-based experimental toolchain that parses, compiles, and runs TypeScript and JavaScript programs as native machine code. RTS is an active compiler/runtime project—not a drop-in replacement for Node.js or a finished browser runtime.

[![Cranelift](https://img.shields.io/badge/backend-Cranelift-orange?style=flat-square)](https://cranelift.dev)
[![Rust](https://img.shields.io/badge/runtime-Rust-black?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Bun/Node parity](https://img.shields.io/badge/Bun%2FNode%20parity-72.9%25-yellowgreen?style=flat-square)](#cross-runtime-parity)

</div>

> **Project status:** RTS is under active development. The sections below describe the current tree and deliberately distinguish verified behavior from roadmap work.

## What RTS is

RTS is a compiler and runtime for TypeScript and JavaScript written in Rust. The language front end, machine layer, runtime, and host are separate components with explicit boundaries. The compiler emits an intermediate representation, the machine layer lowers that representation through Cranelift, and the host connects compiled code to the runtime.

The project is designed around a simple division of responsibility:

| Component | Responsibility |
|---|---|
| `rts-codegen` | JavaScript/TypeScript syntax, parsing bridge, semantic checks, and emission to RTS IR. |
| `rts-cranelift` | Machine-level IR, representations, object layout, GC contract, frames, calls, unwinding, and lowering to Cranelift. |
| `rts-core` | Runtime values, objects, text, memory, coercion, collection, scheduling, and entry points. |
| `rts-host` | The integration point that wires the language, machine, and runtime together and executes programs. |
| `rts-std` / `rts-node` | The `rts:` and `node:` compatibility surfaces. |
| `rts-runtime` / `rts-linker` | The runtime archive and native linking required by AOT compilation. |
| `rts-cli` | Command-line dispatch and project operations. |
| `rts-egui`, `rts-dom`, `rts-render`, `rts-input`, `rts-ui` | Experimental windowing, DOM, rendering, input, and UI capabilities. |
| `rts-napi` | N-API compatibility for loading native `.node` addons. |

The canonical architecture is documented in [`docs/engine/architecture.md`](docs/engine/architecture.md). The binding rules for the two central compiler crates live in [`crates/rts-codegen/README.md`](crates/rts-codegen/README.md) and [`crates/rts-cranelift/README.md`](crates/rts-cranelift/README.md).

## Current status

RTS has a working execution path for a growing JavaScript/TypeScript subset, including classes, closures, modules, async/generator constructs, objects, property access, regular expressions, promises, error handling, and a growing set of built-ins. The project measures progress with executable fixtures rather than relying only on feature checklists.

The latest status recorded in the repository reports **754 of 808 runtime fixtures passing** on 2026-08-15. A separate cross-runtime corpus compares standalone TypeScript output with Bun and Node; its scope, exclusions, and reproduction instructions are documented in [`tests/cross-runtime/README.md`](tests/cross-runtime/README.md). These figures are dated measurements, not a claim of full ECMAScript or Node.js conformance.

| Area | Current position | Source of truth |
|---|---|---|
| JavaScript/TypeScript execution | Broad and expanding; the runtime suite is the primary executable measure. | [`CLAUDE.md`](CLAUDE.md) and [`crates/rts-host/tests/running.rs`](crates/rts-host/tests/running.rs) |
| Front-end grammar | The syntax tree and parser bridge are measured against test262 input. | [`crates/rts-codegen/README.md`](crates/rts-codegen/README.md) and [`crates/rts-codegen/PLAN.md`](crates/rts-codegen/PLAN.md) |
| `node:` compatibility | Several modules are fully verified, while many others remain partial or specification-only. | [`docs/reference/node/node_completed.md`](docs/reference/node/node_completed.md) |
| HTML/CSS and UI | Experimental and intentionally scoped; it is not a general-purpose browser engine. | [`docs/ui/html-engine/README.md`](docs/ui/html-engine/README.md) |

## Install from source

A source checkout currently provides the most complete development path. Install a stable [Rust toolchain](https://www.rust-lang.org/tools/install), Git, and the platform dependencies required by the workspace. On Linux, the CI build installs `libasound2-dev`, `pkg-config`, and `jq`; Windows builds use the MSVC toolchain. macOS builds are exercised by CI on Apple Silicon.

```bash
git clone https://github.com/UrubuCode/rts.git
cd rts
cargo build --release
```

The optimized executable is written to `target/release/rts` on Unix-like systems and `target/release/rts.exe` on Windows. During development, prefer `cargo run` and crate-scoped checks instead of repeatedly rebuilding the whole release workspace.

## Run a program

The smallest reproducible example does not depend on a repository fixture:

```bash
cat > hello.ts <<'EOF'
console.log("hello from RTS")
console.log(1 + 2)
EOF

cargo run -- run hello.ts
cargo run -- -e 'console.log(6 * 7)'
```

After a release build, the same commands are:

```bash
target/release/rts run hello.ts
target/release/rts -e 'console.log(6 * 7)'
```

On Windows, use `target/release/rts.exe` instead of `target/release/rts`.

## Command-line interface

The CLI is intentionally small and is implemented in [`crates/rts-cli/src/cli/mod.rs`](crates/rts-cli/src/cli/mod.rs).

| Command | Purpose |
|---|---|
| `rts run <file.ts>` | Compile to executable memory and run a source file. |
| `rts -e <source>` | Evaluate inline TypeScript/JavaScript source. `rts eval` is also accepted. |
| `rts compile <file.ts> <output>` | Build an AOT executable from a source file. Use `-p`/`--production` for the production profile. |
| `rts test [path]` | Run one fixture or the runtime test suite. |
| `rts ir <file.ts>` | Print RTS IR without executing the program. |
| `rts init [name]` | Scaffold a project. |
| `rts clean` | Remove generated project artifacts handled by the CLI. |
| `rts emit-types [output.d.ts]` | Emit TypeScript declarations derived from registered native classes. |
| `rts install`, `rts i`, `rts add` | Install packages from a manifest or command-line arguments. |
| `rts napi <file.node>` | Load a native N-API addon. |

Use `rts help` for the current built-in help text. Commands are evolving, so this table intentionally does not describe removed or experimental commands as if they were stable.

## JIT and AOT

RTS exposes two compilation destinations. `run` uses the in-memory path, while `compile` creates a native executable through the linker and the RTS runtime archive.

```bash
# JIT: source is compiled and executed in memory.
target/release/rts run hello.ts

# AOT: source is compiled into a native executable.
target/release/rts compile -p hello.ts hello
./hello
```

The AOT path is currently best treated as a **source-build workflow**. The CI build performs an AOT smoke test immediately after compiling the workspace, while the downloaded release-artifact job does not yet have the runtime archive on disk needed by `rts compile`. This limitation is tracked in the build workflow rather than hidden behind the “single binary” claim that the previous README made.

## Cross-runtime parity

[![Bun/Node parity](https://img.shields.io/badge/Bun%2FNode%20parity-72.9%25-yellowgreen?style=flat-square)](#cross-runtime-parity)

The CI corpus contains standalone TypeScript fixtures that run in Bun, Node, and RTS. Output is compared line by line. The current snapshot contains **1,101 passing fixtures out of 1,511 comparable fixtures**, with one additional fixture classified as an upstream defect. The numbers below are dated and should be refreshed from the CI report when the corpus or engine changes.

```text
[▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱] 72.9%   1101/1511 fixtures passing
```

For fixture conventions, exclusions, local execution, and the Bun-versus-Node policy, read [`tests/cross-runtime/README.md`](tests/cross-runtime/README.md). To run the complete corpus locally:

```bash
bash scripts/cross_runtime_check.sh
```


## Node.js compatibility

RTS is not a Node.js replacement yet. The compatibility surface is implemented incrementally and must be verified module by module. The repository's audited tracker currently lists `node:string_decoder`, `node:querystring`, `node:punycode`, `node:os`, `node:path`, `node:url`, `node:net`, and `node:fs` as fully verified. `node:dgram` is near-complete; other modules are partial or not started.

See [`docs/reference/node/node_completed.md`](docs/reference/node/node_completed.md) for the evidence and exact remaining gaps. The full reference index is [`docs/reference/node/INDEX.md`](docs/reference/node/INDEX.md).

## HTML, CSS, and UI

The repository also contains an experimental retained DOM and rendering direction built around `rts-egui`. The current roadmap prioritizes rich text, styled boxes, and interaction, while deliberately leaving broad browser features such as general flexbox, grid, absolute positioning, animations, and modern CSS out of the initial scope.

<img src=".github/imgs/urubu-mascote.png" alt="RTS mascot rendered by the experimental UI engine" width="560" />

Read [`docs/ui/html-engine/README.md`](docs/ui/html-engine/README.md) for the current strategy and [`docs/ui/html-engine/rts-html-roadmap.md`](docs/ui/html-engine/rts-html-roadmap.md) for the operational roadmap. UI examples live under [`examples/`](examples/) and [`site/`](site/).

## Benchmarks

<!-- BENCH_STATS_START -->
### Measured benchmarks

Benchmark results are generated by CI from end-to-end process measurements. They include startup and compilation costs, are platform-specific, and should not be interpreted as universal performance claims. Run the benchmark locally with:

```powershell
./bench/benchmark.ps1
```

The table below is refreshed automatically after the benchmark workflow completes.
<!-- BENCH_STATS_END -->

## Repository map

```text
.
├── crates/
│   ├── rts-codegen/    JavaScript/TypeScript front end
│   ├── rts-cranelift/  machine IR and Cranelift lowering
│   ├── rts-core/      runtime values, objects, heap, and scheduling
│   ├── rts-host/      compiler/runtime integration and executable tests
│   ├── rts-std/       rts: compatibility surface
│   ├── rts-node/      node: compatibility surface
│   ├── rts-runtime/   AOT runtime archive
│   ├── rts-cli/       command-line operations
│   ├── rts-napi/      N-API support
│   └── rts-egui/      DOM/UI engine foundations
├── docs/
│   ├── engine/        compiler and runtime architecture
│   ├── guides/        task-oriented guides
│   ├── reference/     external surfaces implemented against
│   └── ui/            graphical engine documentation
├── examples/           TypeScript and UI examples
├── tests/              runtime, compatibility, and cross-runtime fixtures
├── bench/              benchmark programs and runners
└── scripts/            validation and repository automation
```

The project contains more crates than the abbreviated map above. Use the [Cargo workspace manifest](Cargo.toml) and [`docs/README.md`](docs/README.md) as the authoritative indexes.

## Contributing

Before changing a crate, read its local `README.md`. The repository-wide engineering rules, testing gates, and source-of-truth policy are in [`CLAUDE.md`](CLAUDE.md). Documentation is written in English; development discussion may use the team's working language.

During iteration, use the narrowest useful check:

```bash
cargo check -p rts-codegen
cargo test -p rts-codegen
cargo check -p rts-cranelift
cargo test -p rts-cranelift
cargo run -- run hello.ts
cargo run -- ir hello.ts
```

Before a merge that changes runtime or code generation, follow the release and fixture gates in [`CLAUDE.md`](CLAUDE.md). Do not report a feature as complete based only on a verifier or a compile check; the relevant runtime fixture must execute successfully.

## License

RTS is distributed under the [MIT License](LICENSE). Third-party notices and corpus licensing information are documented in [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).

<div align="center">

Made by [UrubuCode](https://github.com/UrubuCode).

</div>
