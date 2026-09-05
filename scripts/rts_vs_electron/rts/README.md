# RTS vs Electron — lado RTS (AOT)

`app.ts` abre `app.html` (cópia de `../app/index.html`) numa janela `rts:egui`, achando a pasta por `process.execPath` — irmã de `examples/view.ts`.

**Compila e abre a janela**, e as duas frases que estiveram aqui — `rts_runtime.lib` mais velho que o motor, e `.exe` a morrer em "cannot resolve module rts:egui" — estavam corretas quando escritas e já não estão: a primeira era frescura do archive (`cargo build -p rts-runtime`); a segunda foi fechada pelo PR #2671, que ligou `rts:dom`/`rts:egui` na sequência de arranque AOT (hoje `rts_runtime_boot::run`, partilhada por `rts-runtime` e `rts-runtime-jit` — ver `crates/rts-runtime-boot/src/lib.rs`).

**`rts compile` embarca um compilador por omissão** desde o lote `aot-embed-compiler`: liga `rts-runtime-jit`, não `rts-runtime`, a menos que se peça `--sem-compilador`/`--no-compiler`. Isto importa para este app se `app.ts` chegar a correr JS de página em runtime (`runScriptsAt`, um `eval`) — sem o compilador embarcado, esse caminho responde a recusa que `crates/rts-host/README.md` documenta, não um erro de compilação. Ver `docs/engine/architecture.md`, secção "Two AOT archives", para o porquê e o custo.
