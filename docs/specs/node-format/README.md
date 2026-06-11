# Estudo: Suporte a `.node` (Node.js native addons) no RTS

> **Status:** pesquisa em andamento (iniciada 2026-06-11).
> **Autor:** investigação técnica assistida (multi-agente + fontes web verificadas).
> **Objetivo:** entender em profundidade o que é o formato `.node`, como ele
> funciona no Node.js, e quais divergências fundamentais o RTS enfrenta caso
> queira dar suporte a carregar/rodar addons nativos `.node` do ecossistema npm.

## Por que este estudo existe

O RTS compila TypeScript para binário nativo com runtime Rust mínimo e ABI de
"tipos de máquina" (`extern "C"`, sem `JsValue`, sem boxing). Já existe uma
proposta análoga e **estática** — o [`.rtslib`](../rtslib-external-namespaces.md)
(objeto `.o` por triple, linkado em compile-time). O `.node` é o **oposto**:
biblioteca dinâmica carregada em runtime via `dlopen`, acoplada à ABI N-API
(que assume um host estilo V8). Suportar `.node` significa reconciliar dois
modelos de execução muito diferentes.

## Índice dos documentos

| Documento | Conteúdo |
|---|---|
| [`01-formato-binario.md`](01-formato-binario.md) | O que é fisicamente um `.node` (PE/ELF/Mach-O), como o Node carrega, símbolo de entrada, struct `node_module`, `NODE_MODULE_VERSION` |
| [`02-napi-abi.md`](02-napi-abi.md) | A ABI N-API/Node-API: `napi_value`, `napi_env`, handle scopes, ciclo de vida, callbacks, refs/finalizers, o núcleo de funções |
| [`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md) | Quanto um addon depende de V8 vs N-API, libuv/event loop, símbolos que o host deve exportar, delay-load no Windows |
| [`04-precedente-bun-deno.md`](04-precedente-bun-deno.md) | Como Bun (sobre JSC) e Deno (sobre V8) implementaram N-API; o que funciona/quebra; custo real |
| [`05-divergencias-rts.md`](05-divergencias-rts.md) | As divergências fundamentais RTS × `.node`: valor, ABI, GC, event loop, JIT/AOT — bloqueador vs engenharia |
| [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md) | Estratégias de implementação, núcleo 80/20, `NODE_MODULE_VERSION`/prebuilds, recomendação faseada |
| [`07-conclusao.md`](07-conclusao.md) | Síntese executiva, veredito de viabilidade e próximos passos |

## TL;DR

**Suportar `.node` é viável, mas só pela porta N-API, preferencialmente no modo
JIT, e nunca para addons V8-diretos/NAN.** Não há bloqueador técnico absoluto
(o Bun provou que dá para implementar N-API sobre engine não-V8). As barreiras
reais são: **volume** (~150 funções `napi_*`, faixa ~110-160), a **cauda longa do event loop**
(libuv vs tokio do RTS), e um **conflito filosófico** entre o `dlopen` dinâmico
do `.node` e a promessa self-contained do AOT (`.rtslib`).

**Os 5 fatos que decidem tudo:**
1. Um `.node` é uma DLL/`.so`/`.dylib` comum; entry point = `napi_register_module_v1`.
2. `napi_value`/`napi_env` são **opacos** → o RTS pode mapeá-los à sua `HandleTable` sem V8.
3. N-API é ABI-estável e engine-independente; V8 cru/NAN não → **escopo = N-API puro**.
4. Addons N-API **pulam** a checagem de `NODE_MODULE_VERSION` no load; prebuilds modernos são por platform+arch.
5. O host só precisa **exportar `napi_*`** do seu binário + `dlopen` + `dlsym` — não relinka o `.node`.

**Recomendação:** começar por **JIT + núcleo 80/20** (~40 fns síncronas);
`.rtslib` e `.node` são **complementares** (first-party performante × compat npm).
Detalhes em [`07-conclusao.md`](07-conclusao.md).

**Metodologia:** pesquisa multi-agente (6 eixos, busca web) sobre fontes
primárias (docs + código-fonte do Node, código de Bun/Deno), com **verificação
adversarial em dois runs independentes completos** (115 afirmações verificadas,
**0 refutadas**, ~31 nuances corrigidas e incorporadas). Detalhes e tabela de
veredictos em [`07-conclusao.md`](07-conclusao.md).
