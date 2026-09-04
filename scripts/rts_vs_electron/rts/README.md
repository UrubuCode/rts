# RTS vs Electron — lado RTS (AOT)

`app.ts` abre `app.html` (cópia de `../app/index.html`) numa janela `rts:egui`, achando a pasta por `process.execPath` — irmã de `examples/view.ts`.

**Não compila para um `.exe` funcional hoje.** `rts-compare.exe compile -p app.ts out` recusa por `target/release/rts_runtime.lib` (03-09 12:12) ser mais velho que `crates/rts-core/src/entry/eval_scope.rs` (04-09 01:42) — pede `cargo build -p rts-runtime`, proibido nesta tarefa. Contornando a checagem via `RTS_RUNTIME_RWK_ARCHIVE` (variável já existente no CLI, não uma alteração de fonte) o link passa, mas o `.exe` morre em <1s com `cannot resolve module "rts:egui"`: `crates/rts-runtime/Cargo.toml` não depende de `rts-ui` e `crates/rts-runtime/src/aot/mod.rs` só chama `rts_std::install`/`rts_node::install` — nenhuma versão do arquivo AOT registra `rts:egui`, é uma lacuna arquitetural e não uma questão de frescura.

Falta: `rts-runtime` ganhar a dependência de `rts-ui` + uma chamada a `rts_ui::install(&mut context)` no `start()` de `aot/mod.rs`, depois `cargo build --release -p rts-runtime && cargo build --release`.
