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

- [x] `Array.prototype.flatMap` — `258_array_flatmap.ts`
- [x] `Array.prototype.at` — `259_array_at.ts`
- [x] `Array.prototype.findLast` e `findLastIndex` — `260_findlast.ts`
- [x] `String.prototype.matchAll` com `/g` — `261_string_matchall.ts`
- [x] `String.prototype.replaceAll` — `262_replaceall.ts`
- [x] `String.prototype.at` — `263_string_at.ts`
- [x] `Object.hasOwn(obj, "key")` vs `obj.hasOwnProperty` — `264_object_hasown.ts`
- [x] `Object.fromEntries` — `265_object_fromentries.ts`
- [x] Logical assignment: `x ||= y`, `x &&= y`, `x ??= y` — `266_logical_assignment.ts`
- [x] Optional catch binding: `try{} catch{}` sem variável, vs `catch(e)` — `117_optional_catch_binding.ts`

═══════════════════════════════════════════════════════════
## Class features modernas

- [x] Private fields `#x` em classe — `267_private_fields.ts`
- [x] Private methods `#method()` — `268_private_methods.ts`
- [x] Static blocks `static { ... }` — `269_static_blocks.ts`
- [x] `new.target` — `270_new_target.ts`
- [x] Computed class member names — `271_computed_class_members.ts`

═══════════════════════════════════════════════════════════
## Symbol + iteração

- [x] `Symbol.iterator` custom em classe + `for...of` usando a classe — `272_symbol_iterator_custom.ts`
- [x] `Symbol.asyncIterator` + `for-await-of` em async iterable — `273_symbol_asynciterator.ts`
- [x] `Symbol.toPrimitive` — `274_symbol_toprimitive.ts`
- [x] Generator `yield*` delegation — `275_generator_yieldstar.ts`
- [x] `Generator.prototype.return` e `.throw` — `276_generator_return_throw.ts`

═══════════════════════════════════════════════════════════
## Error handling ES2022

- [x] `new Error("msg", { cause: err })` — `277_error_cause.ts`
- [x] `AggregateError` construct + `.errors` array + iter — `278_aggregate_error.ts`
- [x] try/catch com Error chain — `279_error_chain_cause.ts`

═══════════════════════════════════════════════════════════
## Date / Number / Math edge

- [x] `Date.prototype.setUTC*` family — `280_date_setutc_family.ts`
- [x] `Number.EPSILON` / `MAX_VALUE` / `MIN_VALUE` / `MAX_SAFE_INTEGER` edge — `281_number_edges.ts`
- [x] `Math.imul` (32-bit signed mul wraparound) — `282_math_imul.ts`
- [x] `Math.fround` — `283_math_fround.ts`
- [x] `Math.f16round` — `284_math_f16round.ts`

═══════════════════════════════════════════════════════════
## Web APIs / Globals

- [x] `queueMicrotask` — `285_queuemicrotask_order.ts`
- [x] `setImmediate` — `286_setimmediate.ts`
- [x] `URL.canParse` — `287_url_canparse.ts`
- [x] `AbortSignal.timeout(ms)` + `AbortSignal.any([sigs])` — `288_abortsignal_timeout_any.ts`
- [x] `Headers.prototype.getSetCookie` — `289_headers_getsetcookie.ts`

═══════════════════════════════════════════════════════════
## JSON edge

- [x] `JSON.stringify` com referência circular — `290_json_circular.ts`
- [x] `JSON.stringify(BigInt)` — `291_json_bigint.ts`
- [x] `JSON.stringify` com `toJSON()` method — `292_json_tojson.ts`
- [x] `JSON.parse` com reviver function — `293_json_reviver_this.ts`

═══════════════════════════════════════════════════════════
## Sintaxe / semântica

- [x] Rest params em arrow function — `294_arrow_rest_params.ts`
- [x] Labeled break/continue — `295_labeled_break_continue.ts`
- [x] `delete` operator — `296_delete_operator.ts`
- [x] `void` operator — `297_void_operator.ts`
- [x] Comma operator — `298_comma_operator.ts`
- [x] `arguments` object em fn não-arrow — `299_arguments_object.ts`
- [x] `this` em strict mode top-level — `300_this_strict_top_level.ts`

═══════════════════════════════════════════════════════════
## ES2024+ recentes

- [x] `Map.groupBy(arr, fn)` e `Object.groupBy(arr, fn)` — `301_groupby_dedicated.ts`
- [x] `Set.prototype.union/intersection/difference/symmetricDifference/isSubsetOf/isSupersetOf/isDisjointFrom` — `302_set_ops_dedicated.ts`
- [x] `Array.fromAsync` — `303_array_fromasync.ts`
- [x] `Promise.try(fn)` — `304_promise_try.ts`
- [x] `Iterator.from(iter)` + iterator helpers `.map .filter .take .drop .reduce .toArray` — `305_iterator_helpers.ts`
- [x] `Iterator.prototype.toArray` (já consome iterator) — `306_iterator_toarray.ts`

═══════════════════════════════════════════════════════════
## Coleções fracas / GC

- [x] `WeakRef` — `307_weakref_shape.ts`
- [x] `FinalizationRegistry` — `308_finalization_registry_shape.ts`

═══════════════════════════════════════════════════════════
## Misc útil

- [x] `console.assert(cond, msg)` — `309_console_assert.ts`
- [x] `console.group` / `groupEnd` — `310_console_group.ts`
- [x] `console.table` com array de objetos — `311_console_table.ts`
- [x] `console.dir(obj, { depth })` — `312_console_dir.ts`
- [x] Tagged template raw strings — `313_tagged_template_raw.ts`
- [x] `import.meta` — `314_import_meta.ts`
- [x] Hashbang `#!/usr/bin/env node` na primeira linha — `315_hashbang.ts`
- [x] `structuredClone()` com Map/Set/Date/RegExp + checar `!==` identidade — `316_structuredclone_mixed.ts`

═══════════════════════════════════════════════════════════
## Numéricos sutis

- [x] BigInt literals `10n`, `0xFFn`, comparação BigInt com Number (`1n == 1`, `1n === 1` false) — `317_bigint_literals.ts`
- [x] Numeric separators `1_000_000` em decimal/hex/binary/octal — `318_numeric_separators.ts`
- [x] `**` operator (power) — vs `Math.pow`, com negativos, com BigInt — `319_exponentiation.ts`

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
