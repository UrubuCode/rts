# Auditoria e correcções do RTS

## Escopo

A análise foi refeita sobre a revisão remota actual do repositório, sem assumir que os problemas descritos em issues antigas continuam presentes. Foram alterados apenas problemas confirmados na superfície actual do runtime e no exemplo `examples/rtscraft`.

## Correcções aplicadas

| Área | Correcção | Ficheiros |
|---|---|---|
| Integração do jogo | Porte dos imports históricos para `rts:egui`, `rts:input`, `node:fs` e globais suportados; o jogo deixou de depender de `rts:buffer`, `rts:render`, `rts:time` e `rts:fs`. | `examples/rtscraft/engine/app.ts`, `main.ts`, `raycast.ts`, `bench.ts` |
| Buffers | Framebuffer, estado, mundo e chunks passaram para `Uint8Array`, `Uint32Array` e `Float64Array`, eliminando chamadas de ABI por acesso de célula no hot path do exemplo. | `raycast.ts`, `engine/world.ts`, `engine/render3d.ts`, `main.ts` |
| Altura do mundo | `topY()` usa o heightmap; a contagem por altura é mantida durante edições; `maxH` é recalculado quando o topo global pode ter sido removido; edições sem mudança deixam de invalidar chunks. | `engine/world.ts` |
| Ciclo de vida de entidades | Entidades destruídas são compactadas em batch no fim do update, em vez de permanecerem indefinidamente na lista da cena. Entidades adicionadas durante o update continuam a ser processadas. | `engine/core.ts` |
| Input | Foi adicionado snapshot de input por frame. O jogador e a IA usam a superfície moderna de `Math`, reduzindo leituras repetidas da ABI. | `engine/input.ts`, `game/player.ts`, `game/slime.ts` |
| Framebuffer do backend | `drawImage` mantém uma textura por janela e chama `TextureHandle::set` nos frames seguintes; a textura só é recriada quando as dimensões mudam. | `crates/rts-egui/src/ctx.rs`, `app/mod.rs`, `canvas.rs` |
| Janela | `createAppAt` volta a respeitar a posição inicial através de `setNextWindowPos`. | `examples/rtscraft/engine/app.ts` |

## Problemas não alterados deliberadamente

A oclusão DDA por billboard foi mantida. Ela pode ser um custo relevante quando houver muitas entidades, mas a alteração mudaria o resultado visual e não foi demonstrada como regressão na revisão actual sem um benchmark de entidades em escala. Também não foi introduzido fixed timestep nem uma nova broad phase de colisão: são evoluções de arquitectura, não correcções seguras de baixo risco para este porte.

## Verificação

A workspace passou em `cargo check --workspace --features ui` e a binary `rts` passou em build debug com `cargo build -p rts --features ui`. A suite actual `cargo test -p rts-host --features ui --test ui_surface` terminou com **4 testes aprovados e 0 falhas**.

O benchmark portado arrancou com TypedArrays e executou os quatro presets até `336×224`. O probe isolado confirmou que um frame `384×256` termina com sucesso. O smoke test do jogo permaneceu activo até ser encerrado pelo timeout, com código 124 e sem erro de runtime; isso é esperado para um loop principal de janela que continua a correr.

As medições headless em modo debug não devem ser tratadas como prova de ganho de FPS. Antes da textura persistente, depois do porte para TypedArrays, os quatro presets medidos foram aproximadamente `307,48`, `445,39`, `646,04` e `862,80 ms/frame`. Após a alteração da textura persistente foram `315,97`, `497,64`, `705,89` e `989,59 ms/frame`. A diferença mostra que o benchmark é dominado pelo custo do raycaster/JIT neste ambiente e que a textura persistente é uma correcção de lifetime/alocação, não um ganho CPU demonstrado pelo benchmark headless. A validação visual em janela real continua necessária para medir o benefício do upload GPU.

## Estado final

O resultado é um porte compatível com a superfície actual do RTS, com invariantes do mundo e da cena validados, sem reintroduzir namespaces históricos. O próximo ganho de escala deve ser medido com um cenário de muitas entidades; só depois desse perfil é seguro decidir entre depth buffer partilhado, LOD de billboards ou broad phase adicional.
