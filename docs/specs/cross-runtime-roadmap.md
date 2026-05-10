# Cross-Runtime Coverage Roadmap

Lista viva das fixtures planejadas para `tests/cross-runtime/`. Cada item
vira um arquivo `NN_<descricao>.ts` quando alguém implementar — geralmente
codex ou kiro em batches. Numeração sequencial seguindo o último número
existente em `tests/cross-runtime/`.

> Status de cobertura atual: ver
> [README](../../README.md) (badge auto-atualizado) ou
> `cross_runtime_history/index.json` para tendência.

## Como usar este documento

- **Adicionar uma fixture nova**: marca `[x]` no item, indica o número
  do arquivo `NN_*.ts` que criou.
- **Adicionar feature nova ao roadmap**: append no fim da categoria
  apropriada, ou crie nova seção `═══ NOME ═══`.
- **Item ambíguo/skip por design**: marca `[~]` com nota de motivo
  (ex: "Bun ≠ Node esperado em strict mode default").
- **Item inviável** (RTS-only ou runtime-specific): marca `[!]` com nota
  e move pra `tests/<nome>.test.ts` (suite RTS interna).

## Convenções de fixture

Cada arquivo em `tests/cross-runtime/`:
- TypeScript standalone (sem `import "rts"`, sem `JSON5`/`Bun`/`Deno`/`process`)
- Primeira linha: comentário `// <descrição curta>`
- Output via `console.log("label=" + resultado)`
- Linhas pequenas pra diff legível (não cole JSON gigante)
- Sem APIs não-determinísticas (`Math.random`, `Date.now()`, `setTimeout`
  exceto se for o foco)
- Validar localmente: `bun fixture.ts` = `node fixture.ts` antes de commitar
- 1 fixture cobre 1 feature + 2-3 edge cases

Helper: rodar `bash scripts/cross_runtime_check.sh` para validar nos 3
runtimes antes de PR.

═══════════════════════════════════════════════════════════
## ES2020-2024 essenciais (produção real)

- [ ] `Array.prototype.flatMap` — depth padrão, mapper retorna array vazio, mapper retorna scalar, mapper retorna array de arrays (não achata 2 níveis), mixing
- [ ] `Array.prototype.at` — index positivo, -1, -2, 0, fora do range (undefined)
- [ ] `Array.prototype.findLast` e `findLastIndex` — predicate matching, sem match (undefined / -1)
- [ ] `String.prototype.matchAll` com `/g` — iterar matches, captures, named groups, lastIndex
- [ ] `String.prototype.replaceAll` — string pattern, RegExp /g, replacement function
- [ ] `String.prototype.at` — index neg em string, fora do range, empty string
- [ ] `Object.hasOwn(obj, "key")` vs `obj.hasOwnProperty` — em prototype chain, inherited, próprio
- [ ] `Object.fromEntries` — de Map, de `Array.from(map)`, de array de tuplas, de `Object.entries` roundtrip
- [ ] Logical assignment: `x ||= y`, `x &&= y`, `x ??= y` — em var, em obj.prop, em obj[expr]
- [ ] Optional catch binding: `try{} catch{}` sem variável, vs `catch(e)`, uso de e fora vs dentro

═══════════════════════════════════════════════════════════
## Class features modernas

- [ ] Private fields `#x` em classe — acesso interno, erro ao acessar de fora (try/catch), em subclasse (não compartilha)
- [ ] Private methods `#method()` — chamada interna, this binding, herança
- [ ] Static blocks `static { ... }` — execução em ordem, acesso a private fields da classe
- [ ] `new.target` — em function regular vs com `new`, em arrow (undefined), em derived constructor
- [ ] Computed class member names — `[expr]() {}`, `[Symbol.iterator]() {}`, com `Symbol.for`

═══════════════════════════════════════════════════════════
## Symbol + iteração

- [ ] `Symbol.iterator` custom em classe + `for...of` usando a classe
- [ ] `Symbol.asyncIterator` + `for-await-of` em async iterable
- [ ] `Symbol.toPrimitive` — controle de hint `"number"/"string"/"default"`, coerção em `+` e `String()`
- [ ] Generator `yield*` delegation — delegando para outro generator, com return value
- [ ] `Generator.prototype.return` e `.throw` — comportamento, finally em generator

═══════════════════════════════════════════════════════════
## Error handling ES2022

- [ ] `new Error("msg", { cause: err })` — `e.cause` acessível, em TypeError/RangeError subclasses
- [ ] `AggregateError` construct + `.errors` array + iter
- [ ] try/catch com Error chain — `error.cause` em chained errors

═══════════════════════════════════════════════════════════
## Date / Number / Math edge

- [ ] `Date.prototype.setUTC*` family — setUTCFullYear, setUTCMonth, setUTCDate, setUTCHours etc + getUTC*
- [ ] `Number.EPSILON` / `MAX_VALUE` / `MIN_VALUE` / `MAX_SAFE_INTEGER` edge
- [ ] `Math.imul` (32-bit signed mul wraparound) — casos negativos, overflow
- [ ] `Math.fround` — perda de precisão f32, NaN, Infinity
- [ ] `Math.f16round` — ES2024 half-precision (pode dar bun_node_diverge se versões diferem)

═══════════════════════════════════════════════════════════
## Web APIs / Globals

- [ ] `queueMicrotask` — ordem vs `Promise.resolve().then`, vs `setTimeout(0)`
- [ ] `setImmediate` — Node-only, RTS pode skip (vai virar rts_error legítimo)
- [ ] `URL.canParse` — válidos, inválidos, com base, ES2023
- [ ] `AbortSignal.timeout(ms)` + `AbortSignal.any([sigs])` — ES2024
- [ ] `Headers.prototype.getSetCookie` — ES2023

═══════════════════════════════════════════════════════════
## JSON edge

- [ ] `JSON.stringify` com referência circular — try/catch TypeError
- [ ] `JSON.stringify(BigInt)` — throws TypeError
- [ ] `JSON.stringify` com `toJSON()` method — em classe custom, em Date (`.toISOString()`)
- [ ] `JSON.parse` com reviver function — transformação de valores, this binding

═══════════════════════════════════════════════════════════
## Sintaxe / semântica

- [ ] Rest params em arrow function — `(a, ...rest) => rest.length`
- [ ] Labeled break/continue — `outer: for { for { break outer } }`
- [ ] `delete` operator — em property, em array index (deixa hole, length não muda)
- [ ] `void` operator — `void 0`, `void expr` (sempre undefined)
- [ ] Comma operator — `(a, b, c)` avalia tudo, retorna último
- [ ] `arguments` object em fn não-arrow — `arguments.length`, `arguments[0]`, `arguments[Symbol.iterator]`
- [ ] `this` em strict mode top-level — undefined em strict, global em sloppy (vai virar bun_node_diverge talvez)

═══════════════════════════════════════════════════════════
## ES2024+ recentes

- [ ] `Map.groupBy(arr, fn)` e `Object.groupBy(arr, fn)` — agrupamento por chave
- [ ] `Set.prototype.union/intersection/difference/symmetricDifference/isSubsetOf/isSupersetOf/isDisjointFrom` — ES2025
- [ ] `Array.fromAsync` — de async iterable, com mapper, com Promise array
- [ ] `Promise.try(fn)` — ES2025, sync/async error catch
- [ ] `Iterator.from(iter)` + iterator helpers `.map .filter .take .drop .reduce .toArray`
- [ ] `Iterator.prototype.toArray` (já consome iterator)

═══════════════════════════════════════════════════════════
## Coleções fracas / GC

- [ ] `WeakRef` — `deref()` retorna obj enquanto strong ref existe, undefined após GC (não testar GC real, só API shape)
- [ ] `FinalizationRegistry` — register + callback shape (sem forçar GC, só verifica API existe)

═══════════════════════════════════════════════════════════
## Misc útil

- [ ] `console.assert(cond, msg)` — falsy assertion behavior
- [ ] `console.group` / `groupEnd` indentação
- [ ] `console.table` com array de objetos
- [ ] `console.dir(obj, { depth })`
- [ ] Tagged template raw strings — `String.raw`, tag function custom recebendo strings + expressions
- [ ] `import.meta` — em ESM module top-level (pode dar bun_node_diverge, mas vale testar)
- [ ] Hashbang `#!/usr/bin/env node` na primeira linha — ES2023 oficial
- [ ] `structuredClone()` com Map/Set/Date/RegExp + checar `!==` identidade

═══════════════════════════════════════════════════════════
## Numéricos sutis

- [ ] BigInt literals `10n`, `0xFFn`, comparação BigInt com Number (`1n == 1`, `1n === 1` false)
- [ ] Numeric separators `1_000_000` em decimal/hex/binary/octal
- [ ] `**` operator (power) — vs `Math.pow`, com negativos, com BigInt

═══════════════════════════════════════════════════════════
## Sugestões futuras (sem prioridade)

Áreas que podem virar fixture quando algum bug aparecer:

- Tail call optimization em recursão profunda (RTS tem TCO)
- Tagged unions / discriminated unions com switch+typeof
- Decorators (Stage 3) — `@decorator class`
- `using` declarations (ES2024 resource management)
- `await using` (ES2024)
- `Atomics.waitAsync` (ES2024)
- `String.prototype.isWellFormed` / `toWellFormed` (ES2024) — já tem mas pode expandir
- Regex `v` flag (Unicode sets, ES2024)
- Regex backreferences nomeadas `\k<name>`
- Regex lookbehind `(?<=)` / `(?<!)`
- `Atomics.notify` / wait com SharedArrayBuffer (cross-thread)
- `Worker` thread API
- `Performance.now()` monotonicidade
- `MessageChannel` ping-pong
- `Intl.RelativeTimeFormat`, `Intl.PluralRules`, `Intl.ListFormat`, `Intl.DisplayNames`
- `Intl.Locale` object API
- Object property descriptors completos (`writable/enumerable/configurable` combos)
- Proxy traps menos comuns (`isExtensible`, `preventExtensions`, `getPrototypeOf`)
- Reflect.ownKeys ordem
- `Function.prototype.toString` formato (pode dar bun_node_diverge)
- Sloppy mode features (não-strict): with statement, hoisting bizarro

═══════════════════════════════════════════════════════════
## Backlog de bugs RTS conhecidos sem fixture

Bugs já descobertos via probing mas que ainda não viraram fixture cross-runtime.
Quando o fix entrar, criar fixture pra evitar regressão:

- [ ] `Number(null)` → RTS NaN, JS 0
- [ ] `parseInt("abc")` → RTS i64::MIN, JS NaN
- [ ] `[null,1,null].join("/")` → RTS "0/1/0", JS "/1/"
- [ ] `let p: number = 8080; "" + p` → RTS "8176" (fcvt bug)
- [ ] Default param referenciando outro param (#640) — `function f(a, b = a)`
- [ ] Mutable closures via `let` captured (#195)
