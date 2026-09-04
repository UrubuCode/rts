# RTS vs Electron — lado RTS (AOT)

`app.ts` abre `app.html` (cópia de `../app/index.html`) numa janela `rts:egui`, achando a pasta por `process.execPath` — irmã de `examples/view.ts`.

## Estado (2026-09-04): o `.exe` ARRANCA, mas o JS da página não corre nele

`rts compile --all-namespaces -p app.ts out` produz um `.exe` que abre a janela "RTS vs Electron" em ~220 ms (~105 MB de working set) e pinta o HTML/CSS estático da página — igual ao que o motor JIT (`rts run`) mostra para o mesmo ficheiro. A `--all-namespaces` é necessária: sem ela o link cai em DCE e o `.exe` morre em runtime com `cannot resolve module "rts:egui"` (a lacuna arquitetural que existia até esta data — `rts-runtime` não linkava `rts-ui`; corrigido pela PR #2671, que passou a registar `rts:egui`/`rts:dom` no runtime AOT).

**O que ainda falta: os `<script>` da página.** `app.ts` chama `runScriptsAt(doc, "https://localhost/")` — a mesma função que `examples/claude-react-janela.ts` usa no lado JIT — mas essa chamada compila o CORPO do `<script>` em RUNTIME (`new Function` → pipeline swc→HIR→JIT, `DomScope.run` em `crates/rts-dom-bridge/src/scope.rs`), e um binário AOT não leva esse compilador consigo: `rts compile` só gera código nativo para o que o PRÓPRIO `app.ts` referencia estaticamente, nunca para texto que só existe dentro de um `<script>` lido de um HTML em runtime. O sintoma medido:

```
[page] <script> 0 de https://localhost/ falhou: a fonte não compilou
```

repetido uma vez por `<script>` da página (três, nesta app — os bundles React/ReactDOM UMD e o componente). A janela abre e fica em branco para ESTA app especificamente, porque ela é montada inteiramente por esses `<script>`; o HTML/CSS estático de uma página SEM JS de página continuaria a pintar normalmente no mesmo `.exe`.

Isto não é uma regressão de `runScriptsAt` — é o mesmo limite que já existia antes da chamada ser adicionada, só que antes ficava silencioso (a página simplesmente nunca tentava correr o próprio JS). Chamar a função na mesma, em vez de a omitir, é o que torna o `.exe` comparável ao lado JIT e o que faz o erro real aparecer no stderr medido em vez de ser escondido por omissão.

## Como reproduzir

```powershell
target\release\rts.exe compile --all-namespaces -p scripts\rts_vs_electron\rts\app.ts <destino>\app
Copy-Item scripts\rts_vs_electron\app\index.html <destino>\app.html
<destino>\app.exe
```

`scripts/rts_vs_electron/medir.mjs` faz exatamente isto (via `RTS_VS_ELECTRON_RTS_EXE` a apontar para o `.exe` já compilado) e lê `js_da_pagina`/`razao_js_da_pagina` do stderr real da corrida — nunca escritos à mão.

## O que falta para o JS da página correr no AOT

Nada disto está ao alcance de recompilar o motor sem `cargo build` (proibido por tarefa nesta medição) — é trabalho de crate, não de configuração:

- Ou `rts compile` passa a levar o pipeline de compilação (swc→HIR→JIT) para dentro do binário AOT, o que significa incluir `rts-codegen`/`rts-cranelift` no `rts-runtime` — um custo de tamanho e de superfície que hoje o AOT existe precisamente para evitar;
- Ou o comparativo com o Electron aceita a distinção como estrutural: um `.exe` AOT serve uma app cujo HTML não tem `<script>` de página (ou cujo `<script>` já foi ele próprio compilado estaticamente para dentro do `.ts`, nunca lido como texto de um HTML em runtime) — que é exactamente o caso que `rts.exe` + o `.ts` fonte (o lado JIT do comparativo) cobre hoje.

O lado comparável ao Electron ("Chromium + app.asar") é o `rts.exe` (o binário do MOTOR, com compilador) + a página — não este `.exe` AOT.
