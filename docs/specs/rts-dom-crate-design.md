# `rts-dom` — DOM retido como crate independente (headless)

> Status: **implementado** (2026-06-25). Extraído de `rts-egui`. Permite reuso do
> DOM no TS sem abrir janela, e mantém o `rts-egui` como mero consumidor da árvore.

## Motivação

O DOM (parser HTML + árvore em arena + `NodeId` versionado + query/mutação) nasceu
dentro de `rts-egui`, mas **não tem nada de UI** — é manipulação de uma árvore de
dados. Morando no crate de UI, ele estava preso à janela: o `Dom` vivia no `UiCtx`
e **toda** a API (`querySelector`/`setText`/…) exigia um handle de janela. Isso
impedia dois reusos legítimos:

1. **TS headless** — parsear/consultar/mutar HTML em memória sem render.
2. **Outros backends** — qualquer renderer (não só egui) poder ler a mesma árvore.

## Decisão

Crate novo **`crates/rts-dom`**, dependendo SÓ de `rts-engine` (como `rts-egui`):

- **`dom.rs`** — `Dom` (arena `Vec<Node>`), `NodeId { gen, idx }` versionado
  (invariante 2), `NodeIdx` interno, query O(1) por `#id`/`.class` + pré-ordem por
  tag, mutação (`set_text`/`set_attr`/`create_element`/`append_child`/`remove_node`).
- **`html.rs`** — tokenizer HTML mínimo + `decode_entities` (nomeadas + numéricas).
- **`abi.rs`** — namespace `rts:dom` HEADLESS: store `thread_local` de `Dom`
  avulsos (handle `u64` próprio — o engine NÃO conhece o DOM, doutrina), e os
  membros `parseHtml`/`createDocument`/`free`/`querySelector`/`setText`/`setAttr`/
  `createElement`/`appendChild`/`removeNode`/`rootId`/`dump`.

### Por que store `thread_local` próprio (e não `Entry` do engine)

O `Entry` do `HandleTable` (rts-engine) é a lista FECHADA de variantes que o engine
conhece. Adicionar `Entry::Dom` faria o **engine nomear o DOM** — viola a doutrina
PRIMORDIAL (o DOM é não-primordial; o engine só conhece primitivos). Então o
`rts-dom` mantém seu próprio `thread_local HashMap<u64, Dom>`, exatamente como o
`rts-egui` mantém o `UiCtx` num thread_local fora do `Entry`.

### Convenções da ABI (todas seguidas)

- Símbolos `__RTS_FN_NS_DOM_*` (convenção `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>`).
- Handle de DOM cruza como `u64`; **`NodeId` cruza VERSIONADO num `i64`**
  (`to_abi`/`from_abi`, `(gen<<32)|idx`).
- Sentinela "nenhum" = **`-1`** (invariante 3 — `u64::MAX` não é exato como
  `number`). Regra TS: extrair o retorno para const antes de comparar.
- Nenhum valor polimórfico na borda; strings entram como `StrPtr`.

## Registro (data-driven, o engine não nomeia "dom")

Mesmo padrão do `egui`:

- `rts-runtime`: dep + `pub use rts_dom as dom;` (`namespaces/mod.rs`).
- `rts-codegen-new/registry_build.rs`: uma row na tabela `REGISTER`
  (`Register { label: "dom", run: ns::dom::register, … }`). O front NUNCA escreve
  `"dom"` em control-flow; é um dado na tabela.
- Os `fn_ptr` reais nos `Member` são colhidos (harvest) e instalados no JIT pelo
  `adapter_symbols`, como qualquer namespace.

## Como o `rts-egui` consome

O `rts-egui` depende de `rts-dom` e faz `pub(crate) use rts_dom as dom;` — assim
`crate::dom::Dom`/`NodeId`/`parse_html_to_dom` seguem resolvendo em `ctx.rs`/
`frame/render.rs`/`widgets.rs` SEM mudar cada call site. O `UiCtx` guarda um
`rts_dom::Dom`; o render lê a árvore direto (não pela ABI). As fns `egui.*` de DOM
(com handle de janela) permanecem como atalho ergonômico que opera sobre o `Dom`
do `UiCtx` — um caminho paralelo à ABI headless de `rts:dom`.

```
crates/rts-dom/        DOM puro + ABI headless rts:dom   ← reuso TS sem janela
   ↑ (consome o tipo Dom)
crates/rts-egui/       só o RENDER (frame/render.rs lê rts_dom::Dom)
```

## Validação

- `rts-dom`: 27 testes (árvore/parser/entidades + 3 da ABI headless).
- `rts-egui`: 12 testes (style/box) — segue renderizando consumindo `rts-dom`.
- E2E headless: `examples/claude-dom-headless.ts` (parse/query/mutação/create sem
  janela).
- E2E render: `examples/claude-egui-box-complexo.ts` (egui desenha a árvore).

## Futuro (fora deste escopo)

- Fachada ergonômica `Document`/`Element` em TS sobre `rts:dom` (planejada no F3
  do roadmap do motor web, invariante 5 — lib `.ts` via prelude).
- `getText(node) → Handle` (leitura de texto) — pré-requisito da fachada.
```
