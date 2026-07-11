# RTS Node.js Implementation — Crate Selection

> The vetted third-party Rust crates for implementing the Node.js 25 API in
> `rts-node` (and the `rts-engine` async / `rts-primitives` foundation). Every crate was
> researched against crates.io / docs.rs / GitHub and **adversarially verified**
> for license + provenance. Companion: [`implementation-plan.md`](./implementation-plan.md).

## 1. License policy (owner-set, 2026-07-09)

Accept any license that **does not affect the project** (no viral copyleft on our
code) and **requires no royalty/fee**:

- **Accept:** MIT · Apache-2.0 (± LLVM-exception) · BSD-2/3-Clause · ISC · Zlib ·
  Unicode-3.0 · **MPL-2.0** (file-level weak-copyleft — does not touch our code) ·
  BSL-1.0 (Boost) · CC0-1.0 / Unlicense (public domain) · CDLA-Permissive-2.0
  (permissive data license). Attribution-only obligations (keep a NOTICE) are fine.
- **Reject:** GPL / LGPL / AGPL (copyleft affects the project) · BUSL-1.1 / SSPL /
  Commons-Clause / proprietary (payment or use restriction).

Under this policy **none of the researched crates is license-rejected** — every
candidate is permissive or weak-copyleft-without-royalty. Licenses worth a NOTICE
entry (not a blocker): `webpki-roots` (CDLA-Permissive-2.0), `idna`→ICU4X
(Unicode-3.0), `moka` (compound Apache-2.0 portion), `ed25519/x25519/curve25519`
(BSD-3), `whoami` (BSL-1.0), `colored` (MPL-2.0, already in tree), `notify` (CC0).

## 2. Purity bar (what "pure Rust, not a binding" means here)

The filter that actually rejects crates is **provenance**, applied consistently
with what RTS already does:

- **REJECT** — a crate that **compiles or vendors C/C++** (`cc`/`bindgen` build
  step) or **links a third-party C library** (`*-sys` around a real C lib).
  Examples rejected below: `ring`, `aws-lc-rs`, `zstd`/`zstd-sys`,
  `libsqlite3-sys`/`rusqlite`, `wasmtime` (cc), `security-framework`/`schannel`.
- **ACCEPT** — pure-Rust logic that reaches the OS only through **syscall/WinAPI
  FFI *declaration* crates** (`libc`, `windows`/`windows-sys`, `linux-raw-sys`,
  `rustix`, `getrandom`, `mio`). No C is compiled; this is exactly the sanctioned
  pattern CLAUDE.md already uses (`BCryptGenRandom`, `GetThreadContext`,
  `SuspendThread`). `tokio`, `rustix`, `sysinfo`, `hyper`, `hickory` all sit here.

> Note: the verify pass sometimes applied a stricter "any `libc` = impure" reading
> (see `crossterm`/`rustyline` under §5). This doc uses the consistent bar above:
> `libc`-as-FFI-declaration is **accepted** (the project already links it); only a
> C **compile/vendor** step is rejected. Where that changes a verdict it is noted.

## 3. Already in the workspace (reuse first)

| Crate | Ver | License | Relevance to Node impl |
|---|---|---|---|
| tokio | 1.52 | MIT | async runtime → moves into `rts-engine` (native-async feature) |
| rustls | 0.23 | Apache/ISC/MIT | node:tls/https core — **pure-Rust CryptoProvider from RustCrypto, no `ring`** (§6) |
| webpki-roots | 1.0 | CDLA-Permissive-2.0 | node:tls default CA store |
| sha2 | 0.10 | MIT/Apache | node:crypto SHA-2 |
| flate2 | 1.1 | MIT/Apache | node:zlib gzip/deflate (miniz_oxide backend) |
| regex / fancy-regex | 1.x / 0.13 | MIT/Apache · MIT | JS RegExp (engine) |
| indexmap | 2.x | MIT/Apache | ordered maps; h2/hyper reuse it |
| rayon | 1.x | MIT/Apache | parallelism |
| serde_json | 1.x | MIT/Apache | JSON |
| rustix | 1.1 | Apache-LLVM/Apache/MIT | tty/os syscalls (transitive today) |
| url · idna · percent-encoding · form_urlencoded | 2.5/1.1/2.3/1.2 | MIT/Apache | node:url/querystring/punycode (transitive today) |
| encoding_rs | 0.8 | Apache/MIT + BSD-3 (data) | TextDecoder non-UTF-8 (transitive today) |
| bytes · mio · getrandom · tempfile · glob | — | MIT/Apache | async-fs-misc (transitive today) |
| notify | 6.1 (bump→8.2) | CC0 | fs.watch (declared, unused) |
| zstd | 0.13 | MIT wrapper / **C via zstd-sys** | build-tooling only — **do NOT reuse for node:zlib** (§5) |

## 4. Vetted crates by capability domain

All entries below: **pure Rust** (per §2), **license-accepted** (per §1). "ws" =
already resolvable in the workspace.

### 4.1 crypto — hash / MAC / KDF / random  → `rts-node` (algorithms promotable to `rts-primitives`)
| Crate | Ver | License | Covers |
|---|---|---|---|
| sha2 (ws) | 0.10/0.11 | MIT/Apache | SHA-224/256/384/512 |
| sha1 | 0.11 | MIT/Apache | SHA-1 |
| md-5 | 0.11 | MIT/Apache | MD5 (import `md5`) |
| sha3 | 0.12 | MIT/Apache | SHA3/Keccak |
| shake | 0.1 | MIT/Apache | SHAKE128/256 (split out of sha3 0.12) |
| blake2 | 0.10 | MIT/Apache | BLAKE2b/2s |
| ripemd | 0.2 | MIT/Apache | RIPEMD-160 |
| digest · hmac · hkdf · pbkdf2 · scrypt | 0.11/0.13/0.13/0.13/0.12 | MIT/Apache | trait plumbing + HMAC + KDFs |
| getrandom | 0.4 | MIT/Apache | randomBytes/randomFillSync/getRandomValues |
| uuid | 1.x | MIT/Apache | randomUUID (or hand-roll, zero-dep) |

### 4.2 crypto — symmetric ciphers  → `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| aes · cbc · ctr · ecb | 0.9/0.2/0.10/0.2 | MIT/Apache (ecb MIT) | AES + CBC/CTR/ECB modes |
| aes-gcm · chacha20poly1305 | 0.11/0.11 | Apache/MIT | AEAD (NCC-audited) |
| aes-kw | 0.3 | MIT/Apache | AES Key Wrap |
| cipher · aead | 0.5/0.6 | MIT/Apache | shared traits (transitive) |

*ECB is insecure-by-design — document, mirror Node's guidance. `ecb` is outside the RustCrypto org (single maintainer) but long-lived/widely used.*

### 4.3 crypto — asymmetric / X.509  → `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| rsa | 0.9 | MIT/Apache | RSA PKCS1v15/OAEP/PSS (⚠ **RUSTSEC-2023-0071** Marvin timing, no patch — document) |
| pkcs1 · pkcs8 · spki · sec1 · der | 0.8-rc/0.11/0.8/0.8/0.8 | Apache/MIT | key format I/O |
| elliptic-curve · ecdsa | 0.14/0.17 | Apache/MIT | generic EC + ECDSA |
| p256 · p384 · p521 | 0.14 | Apache/MIT | NIST curves |
| ed25519-dalek · x25519-dalek · curve25519-dalek | 3.0/3.0/5.0 | BSD-3 | EdDSA / X25519 |
| signature | 3.0 | Apache/MIT | Signer/Verifier traits |
| x509-parser · x509-cert | 0.18/0.3 | MIT/Apache | cert parse (default features only — never `verify`/`verify-aws`) |
| crypto-bigint | 0.7 | Apache/MIT | hand-roll classic MODP DH |

### 4.4 compression (node:zlib + CompressionStream)  → `rts-node` (algorithms shareable)
| Crate | Ver | License | Covers |
|---|---|---|---|
| flate2 (ws) | 1.1 | MIT/Apache | gzip/deflate/raw + CompressionStream gzip/deflate — **keep default `rust_backend` (miniz_oxide); never enable `zlib`/`zlib-ng`/`cloudflare_zlib` (all pull C)** |
| brotli | 8.0 | BSD-3 + MIT | brotliCompress/Decompress (Dropbox pure-Rust) |
| ruzstd | 0.8 | MIT | zstdCompress/Decompress (encoder trails libzstd in ratio/speed — accepted pure-Rust cost) |

### 4.5 dns (node:dns)  → `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| hickory-resolver | 0.26 | MIT/Apache | lookup/resolve*/reverse — **default features only** (no `dnssec-*`/`tls-*`/`quic-*`/`h3-*`, which pull ring/quinn) |
| hickory-proto · hickory-net · moka | 0.26/0.26/0.12 | MIT/Apache | transitive (RDATA types, transport, cache) |

*resolveCname/Naptr/Ptr/Caa/Any have no named method — decode via `hickory-proto` RData on the generic `lookup()`.*

### 4.6 http / https (node:http, node:https)  → `rts-node` (independent of actix)
| Crate | Ver | License | Covers |
|---|---|---|---|
| hyper | 1.10 | MIT | HTTP/1.1 client+server — `default-features=false, features=["http1","client","server"]` (keeps h2 out) |
| http · http-body · http-body-util | 1.4/1.0/0.1 | MIT/Apache · MIT | types + streaming bodies |
| hyper-util | 0.1 | MIT | `["client-legacy","server-graceful","tokio"]` only (Agent pooling); **NOT `server-auto` (pulls h2) or `client-proxy-system` (pulls macOS/Win framework FFI)** |
| tokio-rustls | 0.26 | MIT/Apache | async TLS for https — `default-features=false`; install the **pure-Rust RustCrypto CryptoProvider** (§6), not ring/aws-lc-rs |
| httparse | 1.10 | MIT/Apache | optional leaner parser (redundant with hyper) |

### 4.7 http2 (node:http2 — defer)  → `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| h2 | 0.4 | MIT | HTTP/2 (inline HPACK, streams, push). Add only when node:http2 is scoped. |

### 4.8 url / idna / querystring / punycode  → `rts-primitives` (cross-context)
| Crate | Ver | License | Covers |
|---|---|---|---|
| url (ws) | 2.5 | MIT/Apache | WHATWG URL/URLSearchParams; from/to_file_path |
| idna (ws) | 1.1 | MIT/Apache | UTS-46 + punycode (pulls ICU4X, Unicode-3.0) |
| percent-encoding (ws) | 2.3 | MIT/Apache | encode/decodeURIComponent building blocks |
| form_urlencoded (ws) | 1.2 | MIT/Apache | querystring core (needs a thin shim for array-fold/custom-sep/maxKeys) |

*Legacy `url.parse/format/resolve` is not provided by `url` — hand-build a shim, as Node itself does.*

### 4.9 text-encoding (TextDecoder / string_decoder)  → `rts-primitives`
| Crate | Ver | License | Covers |
|---|---|---|---|
| encoding_rs (ws) | 0.8 | Apache/MIT + BSD-3 (data) | full WHATWG label set. **Legacy Buffer `latin1`/`binary` = raw byte cast, NOT encoding_rs windows-1252** |
| codepage | 0.1 | Apache/MIT | optional: numeric Windows codepage IDs → Encoding |

### 4.10 os / sysinfo (node:os + process resource usage)  → `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| sysinfo | 0.39 | MIT | cpus/mem/loadavg/uptime/hostname/type/release/networkInterfaces (no `*-sys` even) |
| rustix (ws) | 1.1 | Apache-LLVM/Apache/MIT | Unix uid/gid, get/setPriority |
| whoami | 2.1 | Apache/BSL-1.0/MIT | userInfo username/realname |

*`process.cpuUsage()`/`resourceUsage()` user/system split: hand-roll `libc::getrusage` (Unix) + `GetProcessTimes`/`GetProcessMemoryInfo` (Windows), matching the project's raw-FFI convention. Skip `num_cpus` (`std::thread::available_parallelism`) and `home` (`std::env::home_dir`, correct since Rust 1.87).*

### 4.11 tty / readline / repl  → `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| rustix (ws) | 1.1 | Apache-LLVM/Apache/MIT | isatty (or `std::io::IsTerminal`), termios raw-mode, TIOCGWINSZ |
| terminal_size | 0.4 | MIT/Apache | columns/rows |
| supports-color | 3.0 | Apache | getColorDepth/hasColors |
| crossterm *(optional)* | 0.29 | MIT | full terminal control + keypress events — **acceptable under §2** (pulls `mio`/`signal-hook`/`libc` = FFI-decl, no C compile); the verify pass flagged it under a stricter "no libc" reading |
| rustyline *(optional)* | 18.0 | MIT | ready-made line editor — acceptable under §2; **never enable `with-sqlite-history`** (pulls libsqlite3-sys/cc) |

*Two paths: (a) hand-roll the readline line editor on `rustix` raw-mode (zero-libc-logic, most work); (b) use `crossterm`+`rustyline` (mature, libc-FFI-decl only — consistent with tokio/rustix already in tree). Recommend (b) unless a strict zero-libc build is mandated.*

### 4.12 sqlite (node:sqlite — experimental)  → `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| turso_core | 0.6+ | MIT | pure-Rust SQLite-file-compatible engine — **pin `default-features=false, features=["fs","uuid","time","json","series","percentile","pure-rust-crypto"]`** (stock defaults pull `aegis`→`cc` C-compile on Windows/Linux; `pure-rust-crypto` forces the `softaes` pure backend) |

*Still pre-1.0/BETA — verify SQL surface (triggers/pragmas) before claiming parity. Reject `turso` wrapper (mimalloc C), `limbo` (renamed→turso, frozen), `rusqlite` (libsqlite3-sys/cc), `gluesql` (no SQLite file format).*

### 4.13 wasi (node:wasi — experimental)  → `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| wasmi | 1.1 | MIT/Apache | pure-Rust WASM interpreter (audited: Runtime Verification 2024) |
| wasmi_wasi | 1.1 | MIT/Apache | WASI preview1 host bindings (pulls `wasi-common` sync-only) |

*Reject `wasmtime`/`wasmtime-wasi`: compiles C via `cc` for any real execution, preview1 mandatorily pulls the async/fiber(cc) chain, and vendors its own Cranelift 0.135 fork (RTS pins 0.131). Interpreter ceiling < JIT — the correct pure-Rust tradeoff.*

### 4.14 async / fs-misc (foundation)  → `rts-engine` (async) / `rts-node`
| Crate | Ver | License | Covers |
|---|---|---|---|
| tokio (ws) · mio (ws) | 1.52 · 1.2 | MIT | runtime + reactor → **into `rts-engine`** behind `native-async` |
| tempfile · glob (ws) · globset · bytes (ws) | 3.x/0.3/0.4/1.x | MIT/Apache (globset dual Unlicense) | temp files, glob, byte buffers |
| tokio `signal` feature | — | MIT | process signals (prefer over raw `signal-hook`) |
| etcetera | 0.11 | MIT/Apache | XDG/home dirs — **MPL-free** (prefer over `dirs`/`directories` which pull `option-ext` MPL — MPL is now accepted, but etcetera is copyleft-free) |
| notify (ws) | bump→8.2 | CC0 | fs.watch (also collapses the duplicate `mio 0.8`/`1.2` in the lockfile) |

## 5. Rejected — with reasons

| Crate | Reason (provenance, per §2) |
|---|---|
| **ring** | Compiles C + hand-written **assembly** via `cc` (NASM on Windows). Currently the rustls provider in production — **being dropped**: node:tls uses a pure-Rust RustCrypto `CryptoProvider` instead (§6). |
| aws-lc-rs | rustls's other provider — links C aws-lc via cc/cmake. Not pure. |
| zstd (+zstd-safe/zstd-sys) | Vendors ~41k lines of C libzstd + `cc` + `bindgen`. Already in tree for build-tooling; **must not back node:zlib zstd** — use `ruzstd`. |
| rusqlite / libsqlite3-sys | `bundled` feature compiles vendored C SQLite via `cc`. Use `turso_core`. |
| rcgen | Default features include `ring` (C/asm); no pure-Rust backend. Also out of scope (Node doesn't generate certs). |
| wasmtime / wasmtime-wasi | `build.rs` compiles `helpers.c`/`windows.c` via `cc` for any execution; preview1 forces the async/fiber(cc) chain. Use `wasmi`. |
| rustls-native-certs | Links Apple `Security.framework` / Windows `schannel` (native FFI). Unneeded — Node ships its own CA list (`webpki-roots`). |
| dirs / directories | Pull `option-ext` (**MPL-2.0**, unconditional). MPL is now *accepted*, but `etcetera` is cleaner (no copyleft). |
| getrusage | Abandoned (2019) CLI wrapper, non-standard license field. Hand-roll `libc::getrusage`/`GetProcessTimes`. |
| encoding (rust-encoding) | Frozen since 2017, pre-dates WHATWG updates. Use `encoding_rs`. |
| static-dh-ecdh | Unmaintained (2021); drags a 5-major-versions-stale elliptic-curve stack. Hand-roll DH on `crypto-bigint`. |
| sct | Not actually a rustls dep anymore; Node has no SCT surface. Exclude. |

## 6. Pure-Rust TLS — no `ring` (owner decision)

**Yes, TLS can be pure Rust.** `rustls` delegates all crypto to a pluggable
`CryptoProvider`; only its two *built-in* providers are non-pure (`ring` = C+asm
via `cc`; `aws-lc-rs` = C via cc/cmake). The provider is just a bundle of AEAD +
hash + KX + signature primitives — and RTS **already pulls every one of those as a
pure-Rust RustCrypto crate for node:crypto**:

| rustls needs | pure-Rust crate (already vetted for node:crypto) |
|---|---|
| AEAD | `aes-gcm`, `chacha20poly1305` |
| hash / HKDF / HMAC | `sha2`, `hkdf`, `hmac` |
| key exchange | `x25519-dalek`, `p256`, `p384` (ECDH) |
| signatures | `rsa`, `ecdsa`+`p256`/`p384`, `ed25519-dalek` |
| secure random | `getrandom` |

So node:tls/https ships on a **pure-Rust `CryptoProvider` assembled from those
crates** — no C anywhere in the TLS stack. Two ways to get there:

- **(a) Adopt/harden `rustls-rustcrypto`** — it already wires exactly this bundle
  (all-RustCrypto, verified pure). At 0.0.2-alpha it warns "not for production" (9
  cipher suites, no FIPS), so treat it as a starting point to vendor + harden, not
  a black box.
- **(b) Build our own `CryptoProvider`** from the crates above — more control, same
  crates node:crypto uses, no extra supply-chain surface.

**Recommendation:** target the pure-Rust provider (drop `ring` entirely). Start
with the modern suite set (TLS 1.3 AES-GCM/ChaCha20-Poly1305 + TLS 1.2 ECDHE);
per the "immature-goes-last" rule, treat **full cipher-suite/parity hardening and
constant-time review as end-of-plan work** — basic pure-Rust TLS 1.3 lands early,
exhaustive parity later. Note `rsa`'s RUSTSEC-2023-0071 (Marvin timing) applies to
RSA decryption; TLS 1.3 uses RSA only for signatures, limiting exposure — document
it. `ring` is kept only as an optional bring-up fallback, not the target; the
`TEMP até Fase 2` crypto deps in `rts-engine/Cargo.toml` drain out with this.

## 7. Consolidated new dependencies per crate

- **`rts-engine`** (async primitive): `tokio` + `mio` (existing) behind a
  `native-async` feature. No new C. TLS provider crates
  (`aes-gcm`/`chacha20poly1305`/`sha2`/`hkdf`/`p256`/`x25519-dalek`/`rsa`/`ecdsa`/
  `ed25519-dalek`) are shared with node:crypto (§6).
- **`rts-primitives`** (promotions + greenfield): `url`, `idna`, `percent-encoding`,
  `form_urlencoded`, `encoding_rs` (all existing-transitive → direct); optional
  `codepage`. Pure.
- **`rts-node`** crypto: the RustCrypto/dalek set (§4.1–4.3). net/http:
  `hyper`+`http`+`http-body`+`http-body-util`+`hyper-util`+`tokio-rustls`
  (+`rustls`/`ring` existing). dns: `hickory-resolver`. zlib: `flate2` (existing)
  +`brotli`+`ruzstd`. os: `sysinfo`+`rustix`+`whoami`. tty: `terminal_size`+
  `supports-color` (+`crossterm`/`rustyline` optional). sqlite: `turso_core`.
  wasi: `wasmi`+`wasmi_wasi`. misc: `tempfile`, `glob`/`globset`, `bytes`,
  `etcetera`, `notify` (bump).
- **Cleanup surfaced by the audit:** the lockfile carries duplicate `mio`
  (0.8 + 1.2, collapses when `notify`→8.2) and duplicate `webpki-roots`
  (0.26 via ureq + 1.0); `zstd` is a C-binding kept only for build-tooling.
