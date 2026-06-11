# 06 — Estratégias de implementação e roadmap

> Como o RTS poderia carregar/usar `.node`. Estratégias avaliadas, o núcleo
> 80/20, a questão dos prebuilts/`NODE_MODULE_VERSION`, e uma recomendação
> faseada.

## 6.1 As quatro estratégias

### Estratégia A — N-API completa (espelhar Bun/Deno)

RTS exporta **todos** os símbolos `napi_*`/`uv_*`, carrega `.node` via dlopen,
traduz `napi_value` ↔ handle RTS.
- **Prós:** máxima compatibilidade com o ecossistema N-API.
- **Contras:** ~150 fns (faixa real ~110-160) + paridade comportamental + ponte
  de event loop. Bun levou **anos** e ainda está em ~76% nos suites do Node.
- **Veredito:** é o destino, não o ponto de partida.

### Estratégia B — shim mínimo / núcleo 80/20 ✅ (ponto de partida)

Implementar só o subconjunto que os addons mais usados precisam (~40 fns
síncronas, §6.2). Roda a maioria dos addons N-API simples (parsers, hashing,
compressão síncrona).
- **Veredito:** **melhor primeiro passo** — entrega um addon real rodando cedo.

### Estratégia C — só JIT, nunca AOT ✅ (coerente arquiteturalmente)

Suportar `.node` apenas em `rts run` (JIT: já há memória executável + dlopen
runtime é aceitável); **proibir** em `rts compile` (preserva o self-contained do
`.rtslib`).
- **Veredito:** **a mais coerente** com a arquitetura. Combina com B na fase
  inicial. AOT fica para depois (modelo self-extracting do Deno, opcional).

### Estratégia D — recompilar addon como `.rtslib` ❌ (não resolve compat)

Só viável quando há **source** disponível **e** ele não usa N-API/V8 (a `.rtslib`
usa tipos de máquina + símbolo direto, não `napi_env`/`napi_value`). Para a
maioria dos addons binários do npm seria **reescrita**, não recompilação.
- **Veredito:** a `.rtslib` **não** é rota de compatibilidade com o ecossistema
  `.node` — é um formato nativo **paralelo** (first-party performante). Os dois
  são **complementares**, não substitutos.

## 6.2 O núcleo 80/20 (~40 funções da Estratégia B)

Subconjunto realmente exercido por um addon CRUD/utilitário típico:

- **Registro:** `napi_register_module_v1` + struct `napi_module`.
- **Callbacks:** `napi_create_function`, `napi_get_cb_info`, `napi_call_function`.
- **Criar valores:** `napi_create_double/int32/uint32/int64/bigint`,
  `napi_create_string_utf8`, `napi_create_array`, `napi_create_object`,
  `napi_get_boolean`, `napi_get_undefined`, `napi_get_null`, `napi_get_global`.
- **Extrair:** `napi_get_value_double/int32/string_utf8/bool`,
  `napi_get_array_length`, `napi_get_element`.
- **Propriedades:** `napi_set_named_property`, `napi_get_named_property`,
  `napi_set_property`, `napi_get_property`, `napi_define_properties`.
- **Tipos:** `napi_typeof`, `napi_is_array`, `napi_instanceof`.
- **Erros:** `napi_throw`, `napi_throw_error/type_error`, `napi_create_error`,
  `napi_is_exception_pending`, `napi_get_and_clear_last_exception`.
- **Handle scopes:** `napi_open/close_handle_scope`,
  `napi_open_escapable_handle_scope`, `napi_escape_handle`.
- **Referências:** `napi_create_reference`, `napi_get_reference_value`,
  `napi_delete_reference`, `napi_reference_ref/unref`.
- **Wrap (fase 2):** `napi_wrap`, `napi_unwrap`, `napi_define_class`,
  `napi_add_finalizer`.
- **Instance data:** `napi_set/get_instance_data`.

**Fica para depois:** buffers/typedarrays/arraybuffers (`napi_create_buffer/
external_buffer/typedarray/dataview/arraybuffer`), promises
(`napi_create_promise`, `resolve/reject_deferred`), async work, threadsafe
functions (estes exigem o event loop — degrau difícil).

## 6.3 A questão dos prebuilts e `NODE_MODULE_VERSION`

### Boa notícia: addons N-API pulam a checagem de versão no load

A struct `napi_module` carrega o seu próprio `NAPI_MODULE_VERSION`; a verificação
de `NODE_MODULE_VERSION` do Node **só morde addons não-N-API**. Confirmado
empiricamente: o Bun reporta `NODE_MODULE_VERSION 127` (= Node 22) e addons
N-API carregam sem reclamar de versão (issue `oven-sh/bun#14105`). **O RTS não
precisa fingir um `NODE_MODULE_VERSION` no `dlopen` de um addon N-API.**

### Mas precisa para a fase ANTERIOR: instalação/seleção do prebuilt

`npm`/`prebuild-install`/`node-pre-gyp`/`node-gyp-build` escolhem qual `.node`
baixar por `(platform, arch, abi)`. O RTS reportaria:
- `process.platform` → `win32`/`linux`/`darwin`;
- `process.arch` → `x64`/`arm64`;
- `process.versions.node` → ex.: fingir Node 22;
- `process.versions.modules` → o `NODE_MODULE_VERSION` (ex.: `127`).

Fonte canônica do mapeamento: `doc/abi_version_registry.json` (repo do Node) e o
pacote npm **`node-abi`**.

### Melhor notícia: prebuilds N-API modernos são por (napi|abi)+platform+arch

`prebuildify` nomeia `node.napi.node`, `electron.abi40.node`,
`node.napi.uv1.armv8.node`; `napi-rs` publica `index.darwin-x64.node`,
`snappy.linux-x64-gnu.node` (pacotes scoped por plataforma `@scope/pkg-linux-x64-
gnu`). Pacotes **N-API** publicam **um binário por (plataforma, arch)** — **não**
um por versão de Node (justamente pela estabilidade de ABI).

**Implicação:** addons N-API com prebuild tag `napi` são selecionados por
**platform+arch** e **não** exigem que o RTS finja uma versão de Node específica
para a escolha do binário — só exigem `platform`/`arch` corretos. **Priorizar o
ecossistema `napi-rs`/`prebuildify` minimiza o masquerading.** Pacotes que só
publicam tag `abi<n>` (estilo `prebuild-install` antigo) forçariam o RTS a
reportar um `process.versions.modules` específico.

## 6.4 Modelo de permissão (lição do Deno)

Carregar `.node` roda **código nativo fora do sandbox**. O Deno exige
`--allow-ffi` (aceita lista de paths). **O RTS deveria exigir um flag explícito**
(ex.: `--allow-native-addons` / `--allow-ffi`) para carregar `.node`,
espelhando o Deno — segurança e intenção explícita.

## 6.5 Recomendação faseada concreta

> Cada fase entrega **valor verificável** (um addon real rodando) antes de pagar
> o custo da próxima.

### Fase 0 — Descoberta (loader mínimo, só JIT)
- Interceptar `require('./x.node')` / `import` de `.node` em
  `resolve_node_modules_import` → rotear para loader N-API.
- `libloading` (dlopen/LoadLibrary) + resolver `napi_register_module_v1`.
- `napi_env` mínimo (struct `RtsNapiEnv`) + tradução `napi_value` ↔ handle `u64`.
- **Só** no caminho JIT (`rts run`) — Estratégia C.
- **Garantir export-dynamic** dos símbolos `napi_*` do binário RTS (`.def`/`/EXPORT`
  no Windows; `--export-dynamic`/version script no Linux/macOS).
- **Critério de saída:** um addon N-API trivial (1 função que soma dois números)
  carrega e roda.

### Fase 1 — Núcleo 80/20 (Estratégia B)
- As ~40 fns síncronas do §6.2: valores escalares, `string_utf8`, array, object,
  named properties, `create_function`/`get_cb_info`/`call_function`, `typeof`,
  throw/exceção, handle scopes, references.
- **Critério de saída:** um addon N-API síncrono real roda (ex.: um hashing ou
  compressão síncrona, ou `esbuild` no caminho síncrono).

### Fase 2 — Objetos nativos e GC
- Buffers/typedarrays/arraybuffer + `napi_wrap`/`unwrap`/`define_class`/
  finalizer (integração com GC roots + sweep enfileirando finalizers).
- **Critério de saída:** um addon que expõe uma classe nativa com recurso (ex.:
  um wrapper de DB síncrono) roda e libera recursos corretamente.

### Fase 3 — Async (o degrau difícil)
- Promises (mapear a `promise.create`/#437), async work
  (→ `rt().spawn_blocking`), threadsafe functions (fila MPSC drenada na thread
  JS), `napi_get_uv_event_loop` + **mini-shim de libuv** (`uv_async_t`,
  `uv_loop_t` opaco, `uv_queue_work`) sobre o tokio global.
- Mensagem de erro clara para símbolos `uv_*` não suportados (como o Bun).
- **Critério de saída:** um addon assíncrono (worker em background + callback)
  roda.

### Fase 4 — Distribuição / npm
- Reportar `process.platform`/`arch`/`versions.node`/`versions.modules`
  coerentes para que `npm`/`prebuild-install`/`node-gyp-build` baixem o prebuilt
  certo. Priorizar `napi-rs`/`prebuildify` (selecionados por plataforma).
- **Critério de saída:** `rts i <pacote-com-addon-napi>` baixa o `.node` certo e
  ele carrega.

### AOT — adiado / opcional
- Manter **proibido** em `rts compile` (preserva self-contained), **ou** adotar
  o modelo **self-extracting** do Deno (embutir + extrair para tempdir +
  dlopen), documentado como exceção explícita ao `.rtslib`.

### Nunca
- Addons **V8-diretos / NAN** (fora de escopo, como Bun e Deno). Mensagem de erro
  clara: "addon usa a API C++ do V8, não suportado — use a variante N-API".

## 6.6 Organização de código sugerida (molde Bun/Deno)

Espelhando a separação que funcionou no Bun (`napi.cpp` + `napi.zig`) e a lista
declarativa do Deno (`symbol_exports.json`):

```
crates/rts-runtime/src/napi/
  mod.rs            — RtsNapiEnv, loader (libloading), register handshake
  values.rs         — napi_create_*/napi_get_value_* (↔ HandleTable/gc/collections)
  props.rs          — propriedades, define_properties
  functions.rs      — create_function, call_function, get_cb_info, trampolim callconv
  scopes.rs         — handle scopes (roots extras p/ o GC), references, wrap/finalizers
  errors.rs         — throw, exceção pendente (↔ error slot thread-local)
  async.rs          — async work, threadsafe functions (↔ async_rt/tokio), uv shim
  symbols.rs        — lista declarativa de símbolos N-API exportados (→ .def/export table)
```

A maquinaria que o RTS já tem (geração de `rts.d.ts` a partir de `abi::SPECS`,
convenção `symbols.rs`) geraria a tabela de exports N-API.

## Conclusão do capítulo

- **Ponto de partida: B + C** (núcleo 80/20, só JIT). Destino: A (N-API
  completa).
- A `.rtslib` é **complementar** (first-party performante), não rota de compat.
- Prebuilds N-API por **platform+arch** minimizam o masquerading de versão de
  Node — priorizar `napi-rs`/`prebuildify`.
- Roadmap em 5 fases, cada uma com um **addon real** como critério de saída.

## Fontes

- https://bun.com/docs/runtime/node-api · https://github.com/oven-sh/bun/issues/158 · /14105
- https://nodejs.org/api/n-api.html · /api/addons.html
- https://github.com/nodejs/node/blob/main/doc/abi_version_registry.json
- https://www.npmjs.com/package/node-abi · /prebuildify · /prebuild-install
- https://napi.rs/docs/cli/build · /deep-dive/native-module
- https://docs.deno.com/runtime/fundamentals/node/ · /ffi/ · /reference/cli/compile/
- https://github.com/denoland/deno/issues/23266 · /pull/28934
