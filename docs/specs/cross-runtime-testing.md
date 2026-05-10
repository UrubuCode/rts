# Cross-Runtime Parity Testing

Sistema de testes que valida compatibilidade JS spec do RTS comparando
outputs contra **Bun** e **Node** em fixtures TypeScript standalone.

## Componentes

- **`tests/cross-runtime/*.ts`** — fixtures TS rodáveis em qualquer um dos 3
  runtimes. Sem `import "rts"`, sem `JSON5`/`Bun`/`Deno`/`process`.
- **`scripts/cross_runtime_check.sh`** — roda cada fixture nos 3 runtimes
  (paralelizado via `xargs -P`), compara stdouts, gera JSON.
- **`.github/workflows/cross-runtime.yml`** — CI que roda em PR + schedule.
- **`docs/specs/cross-runtime-roadmap.md`** — lista viva de fixtures
  planejadas (checklist marcavel conforme novos batches sao adicionados).

## Como rodar localmente

```bash
cargo build --release
bash scripts/cross_runtime_check.sh
```

Pre-requisitos: `bun` e `node` no PATH.

## Report JSON consumivel externamente

O CI commita `cross_runtime_report.json` na raiz do repo a cada atualizacao
(push para `main` + schedule semanal). Qualquer dashboard externo pode
consumir via raw URL:

```
https://raw.githubusercontent.com/UrubuCode/rts/main/cross_runtime_report.json
```

Estrutura:

```json
{
  "results": [
    { "name": "01_logical_truthy", "status": "pass",
      "bun": "...", "node": "...", "rts": "..." },
    ...
  ],
  "summary": {
    "total": 107, "pass": 40, "rts_diverge": 22,
    "bun_node_diverge": 0, "errors": 45, "rejected": 0
  }
}
```

Sites/dashboards externos podem fetchar o JSON e renderizar grafico de
progresso da paridade ao longo do tempo (comparar releases diferentes).

## Categorias de output

Cada fixture cai em uma de 5 categorias:

| Status | Significado | Ação |
|---|---|---|
| `pass` | RTS = Bun = Node | ✅ paridade ok |
| `rts_diverge` | RTS ≠ Bun = Node | ❌ bug RTS — abrir issue |
| `bun_node_diverge` | Bun ≠ Node | ⚠️ engine difference — skip |
| `rts_error` | RTS crashou ou panic | ❌ bug RTS |
| `rejected` | Fixture usa API RTS-only | 🚫 mover para `tests/*.test.ts` |

## CI policies

- **Em PR**: roda automaticamente, comenta no PR com tabela de divergências,
  **não bloqueia** merge (opt-in por enquanto, evolver para required quando
  tivermos cobertura ampla).
- **Em schedule semanal** (segunda 6h UTC): se aparecer regressão nova,
  abre issue automática com label `cross-runtime`.
- **Em push para `main`**: roda como sanity check (artifact JSON salvo).

## Dedup de issues por hash

O auto-create de issue em schedule usa **dois níveis de hash** para evitar
duplicatas:

1. **Hash por divergência** (`sig`): SHA-1 truncado de
   `name|status|bun_output|node_output|rts_output`. Mesma assinatura =
   mesmo bug. Inserido no body como `<!-- cross-runtime-sig: <12 chars> -->`
   antes do bloco de outputs.

2. **Hash agregado** (`aggregateHash`): SHA-1 do conjunto de sigs
   ordenadas. Conjunto idêntico de divergências = mesmo hash. Inserido
   no footer do body como `<!-- cross-runtime-hash: <12 chars> -->`.

### Lógica de decisão a cada schedule run

```
divergencias_atuais = run | filter(rts_diverge ou rts_error)
sigs_atuais = map(divergencias_atuais, sig)
hash_atual = sha1(sort(sigs_atuais).join(","))

issues_abertas = labels:cross-runtime

if exists(issue with hash_atual no body):
  # Conjunto idêntico já tem issue — só comenta "ainda presente"
  comment(issue, "🔁 Schedule run YYYY-MM-DD — persistem")
elif all(sigs_atuais já estão em alguma issue aberta):
  # Subconjunto já coberto distribuído em várias issues
  comment(em cada issue afetada)
else:
  # Tem sig(s) inéditos — cria issue nova só com os novos
  create_issue(divergencias_novas, hash_atual no footer)
```

Isso garante:
- **Mesmo bug persistente** → não duplica issue, comentário "ainda presente"
  marca timeline.
- **Conjunto novo de bugs** → issue nova só com os inéditos.
- **Bug novo + bugs antigos** → issue nova só com os novos; antigos ganham
  comentário em suas issues existentes.

### Por que hash truncado de 12 chars

Suficiente para evitar colisão dado o número de fixtures pequeno (centenas
no pior cenário). Espaço de 16^12 = 2.8e14, colisão por aniversário em
~16M divergências distintas — muito além do realista.

## Adicionar fixture nova

1. Criar `tests/cross-runtime/NN_<descrição>.ts` com `console.log` cobrindo
   o comportamento JS que você quer validar.
2. Validar localmente os 3 runtimes batem. Se RTS divergir, é um bug —
   abra fix antes de fazer merge.
3. Commit normal. CI valida no próximo push/PR.

## APIs proibidas em cross-runtime

Estas são RTS-only ou runtime-specific. Script rejeita fixtures que as
usam (regex check pre-execução):

- `import { ... } from "rts"` — namespaces RTS nativos
- `JSON5` — global RTS-only
- `Bun` global — runtime-specific
- `Deno` global — runtime-specific
- `process` global — Node-specific

Se a fixture precisa de qualquer uma, ela vai em `tests/<nome>.test.ts`
(suite RTS via `rts:test`) em vez de cross-runtime.

## Categorias de issue auto-criada

Em vez de criar uma issue gigante com todas as divergências, o workflow
agrupa por **categoria temática** (uma issue por área). Mapeamento atual
em `.github/workflows/cross-runtime.yml` no step "Auto-create issues":

| Categoria | Cobre |
|---|---|
| `regex` | regex methods, named groups, indices, unicode |
| `url` | URL, URLSearchParams |
| `json` | JSON.parse/stringify, replacer/reviver |
| `intl` | Intl.NumberFormat/DateTimeFormat/Segmenter |
| `streams` | ReadableStream, CompressionStream, TextDecoder stream |
| `typed-buffers` | ArrayBuffer, DataView, TypedArray, BigInt, Atomics |
| `web-api` | Blob, File, FormData, Headers, Request/Response |
| `events-async` | AbortController, EventTarget, MessageChannel, microtask |
| `classes-errors` | classes, instanceof, Error family, typeof |
| `promises` | Promise.all/race/withResolvers, async/await |
| `fn-closure-syntax` | closures, function meta, destructuring, templates |
| `array` | array methods, iter, sparse, groupBy, set ops |
| `object-meta` | Object methods, Proxy, Reflect, Symbol |
| `string` | string methods avançados |
| `numeric` | Math, Number format, coercion, bitwise, NaN |
| `date` | Date methods |
| `misc-platform` | WeakRef, structuredClone, dynamic import, etc. |
| `other` | fallback se nome não bate |

Cada categoria com ≥1 divergência inédita gera/atualiza issue própria
com labels `cat:<categoria>` + `cross-runtime` + `bug`.

## Histórico semanal

O CI commita um snapshot em `cross_runtime_history/YYYY-MM-DD.json`
a cada schedule run (1× semana). O arquivo `cross_runtime_history/index.json`
mantém lista cronológica para dashboards consumirem:

```json
{
  "entries": [
    { "date": "2026-05-10", "pct": 37.4, "pass": 40, "total_valid": 107, ... }
  ]
}
```

Snapshots detalhados (`YYYY-MM-DD.json`) trazem nomes dos divergentes
sem outputs completos — economiza espaço a longo prazo.

## Dashboard GitHub Pages

Página `parity.html` no GitHub Pages do projeto consome
`cross_runtime_report.json` + `cross_runtime_history/index.json` e renderiza:

- **% paridade atual** (big number)
- **Stats**: pass / diverge / error / total
- **Gráfico SVG** de evolução de % ao longo das semanas
- **Tabela** com fixtures pendentes

URL final: `https://urubucode.github.io/rts/parity.html`

Atualizado automaticamente quando o workflow cross-runtime termina em
main ou quando `cross_runtime_report.json` muda.

## Bugs cross-runtime conhecidos (track)

Lista vai vivendo aqui conforme aparecem:

- `Number(null)` → RTS NaN, JS 0
- `parseInt("abc")` → RTS i64::MIN, JS NaN
- `[null,1,null].join("/")` → RTS "0/1/0", JS "/1/"
- `var: number = 8080` em concat → RTS 8176 (fcvt bug)

(Esses ainda não viraram fixture porque os fixes são pendentes.)
