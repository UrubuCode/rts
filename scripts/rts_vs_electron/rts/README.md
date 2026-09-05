# RTS vs Electron — lado RTS (AOT)

`app.ts` abre `app.html` (cópia de `../app/index.html`) numa janela `rts:egui`, achando a pasta por `process.execPath` — irmã de `examples/view.ts`.

**Compila e abre a janela**, e as duas frases que estiveram aqui — `rts_runtime.lib` mais velho que o motor, e `.exe` a morrer em "cannot resolve module rts:egui" — estavam corretas quando escritas e já não estão: a primeira era frescura do archive (`cargo build -p rts-runtime`); a segunda foi fechada pelo PR #2671, que ligou `rts:dom`/`rts:egui` na sequência de arranque AOT (hoje `rts_runtime_boot::run`, partilhada por `rts-runtime` e `rts-runtime-jit` — ver `crates/rts-runtime-boot/src/lib.rs`).

## Estado (2026-09-04, depois do lote `aot-embed-compiler`): o JS da página deixa de precisar de um caminho especial

A secção que esteve aqui media exatamente o gap que a lista "O que falta" abaixo descrevia — e a primeira das duas alternativas que ela propunha é o que o lote `aot-embed-compiler` entregou: `rts compile` (sem flag nenhuma) já embarca `rts-codegen`/`rts-cranelift` no binário, ligando `rts-runtime-jit` em vez de `rts-runtime`. `app.ts` chama `runScriptsAt(doc, "https://localhost/")` — a mesma função que `examples/claude-react-janela.ts` usa no lado JIT — e essa chamada agora encontra um compilador instalado (`rts_host::install_compiler`, os mesmos seis ganchos que o JIT já instala) em vez do `context.eval_compiler_with_receiver` vazio que produzia:

```
[page] <script> 0 de https://localhost/ falhou: a fonte não compilou
```

**Não medido de novo NESTE merge** — a reconciliação trouxe o código, `scripts/rts_vs_electron/medir.mjs` (que lê `js_da_pagina`/`razao_js_da_pagina` do stderr real, nunca escritos à mão) é quem tem a palavra final. `--sem-compilador`/`--no-compiler` continua a existir para o lado que quiser voltar a medir o gap antigo de propósito.

Se este app também ganhar um `rts compile --html scripts/rts_vs_electron/app/index.html`, os `<script>`s conhecidos passam a vir PRÉ-compilados do build (`docs/engine/aot-page-scripts.md`) e o compilador embarcado fica como *fallback* apenas para o que `--html` não viu — `crates/rts-runtime-boot/src/page_scripts.rs` tem a composição dos dois.

## Como reproduzir

```powershell
target\release\rts.exe compile -p scripts\rts_vs_electron\rts\app.ts <destino>\app
Copy-Item scripts\rts_vs_electron\app\index.html <destino>\app.html
<destino>\app.exe
```

`--all-namespaces` deixou de ser necessária para `rts:egui` especificamente — `app.ts` já referencia `egui.openWindow` estaticamente, o que a DCE do linker (`/OPT:REF`) já mantém por alcançabilidade normal; ela continua a existir para `import(variable)` e casos parecidos, sem relação com este app.

O lado comparável ao Electron ("Chromium + app.asar") é o `rts.exe` (o binário do MOTOR, com compilador) + a página — este `.exe` AOT é o comparável a um instalador nativo compilado antecipadamente, com ou sem compilador embarcado conforme a flag.
