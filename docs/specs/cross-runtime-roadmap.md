# Cross-Runtime Coverage Roadmap

Living list of fixtures planned for `tests/cross-runtime/`. Each item
becomes a `NN_<description>.ts` file when someone implements it — usually
codex or kiro in batches. Sequential numbering following the last existing
number in `tests/cross-runtime/`.

> Current coverage status: see
> [README](../../README.md) (auto-updated badge) or
> `cross_runtime_history/index.json` for the trend.

## How to use this document

- **Adding a new fixture**: mark the item `[x]`, indicate the number
  of the `NN_*.ts` file you created.
- **Adding a new feature to the roadmap**: append at the end of the appropriate
  category, or create a new `═══ NAME ═══` section.
- **Ambiguous / skip-by-design item**: mark `[~]` with a reason note
  (e.g. "Bun ≠ Node expected in default strict mode").
- **Infeasible item** (RTS-only or runtime-specific): mark `[!]` with a note
  and move it to `tests/<name>.test.ts` (internal RTS suite).

## Fixture conventions

Each file in `tests/cross-runtime/`:
- Standalone TypeScript (no `import "rts"`, no `JSON5`/`Bun`/`Deno`/`process`)
- First line: comment `// <short description>`
- Output via `console.log("label=" + result)`
- Small lines for a readable diff (don't paste giant JSON)
- No non-deterministic APIs (`Math.random`, `Date.now()`, `setTimeout`
  unless it is the focus)
- Validate locally: `bun fixture.ts` = `node fixture.ts` before committing
- 1 fixture covers 1 feature + 2-3 edge cases

Helper: run `bash scripts/cross_runtime_check.sh` to validate on all 3
runtimes before a PR.

═══════════════════════════════════════════════════════════
## Essential ES2020-2024 (real-world production)

- [x] `Array.prototype.flatMap` — `258_array_flatmap.ts`
- [x] `Array.prototype.at` — `259_array_at.ts`
- [x] `Array.prototype.findLast` and `findLastIndex` — `260_findlast.ts`
- [x] `String.prototype.matchAll` with `/g` — `261_string_matchall.ts`
- [x] `String.prototype.replaceAll` — `262_replaceall.ts`
- [x] `String.prototype.at` — `263_string_at.ts`
- [x] `Object.hasOwn(obj, "key")` vs `obj.hasOwnProperty` — `264_object_hasown.ts`
- [x] `Object.fromEntries` — `265_object_fromentries.ts`
- [x] Logical assignment: `x ||= y`, `x &&= y`, `x ??= y` — `266_logical_assignment.ts`
- [x] Optional catch binding: `try{} catch{}` without a variable, vs `catch(e)` — `117_optional_catch_binding.ts`

═══════════════════════════════════════════════════════════
## Modern class features

- [x] Private fields `#x` in a class — `267_private_fields.ts`
- [x] Private methods `#method()` — `268_private_methods.ts`
- [x] Static blocks `static { ... }` — `269_static_blocks.ts`
- [x] `new.target` — `270_new_target.ts`
- [x] Computed class member names — `271_computed_class_members.ts`

═══════════════════════════════════════════════════════════
## Symbol + iteration

- [x] Custom `Symbol.iterator` in a class + `for...of` using the class — `272_symbol_iterator_custom.ts`
- [x] `Symbol.asyncIterator` + `for-await-of` over an async iterable — `273_symbol_asynciterator.ts`
- [x] `Symbol.toPrimitive` — `274_symbol_toprimitive.ts`
- [x] Generator `yield*` delegation — `275_generator_yieldstar.ts`
- [x] `Generator.prototype.return` and `.throw` — `276_generator_return_throw.ts`

═══════════════════════════════════════════════════════════
## ES2022 error handling

- [x] `new Error("msg", { cause: err })` — `277_error_cause.ts`
- [x] `AggregateError` construct + `.errors` array + iter — `278_aggregate_error.ts`
- [x] try/catch with an Error chain — `279_error_chain_cause.ts`

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

- [x] `JSON.stringify` with a circular reference — `290_json_circular.ts`
- [x] `JSON.stringify(BigInt)` — `291_json_bigint.ts`
- [x] `JSON.stringify` with a `toJSON()` method — `292_json_tojson.ts`
- [x] `JSON.parse` with a reviver function — `293_json_reviver_this.ts`

═══════════════════════════════════════════════════════════
## Syntax / semantics

- [x] Rest params in an arrow function — `294_arrow_rest_params.ts`
- [x] Labeled break/continue — `295_labeled_break_continue.ts`
- [x] `delete` operator — `296_delete_operator.ts`
- [x] `void` operator — `297_void_operator.ts`
- [x] Comma operator — `298_comma_operator.ts`
- [x] `arguments` object in a non-arrow fn — `299_arguments_object.ts`
- [x] `this` in strict mode at top level — `300_this_strict_top_level.ts`

═══════════════════════════════════════════════════════════
## Recent ES2024+

- [x] `Map.groupBy(arr, fn)` and `Object.groupBy(arr, fn)` — `301_groupby_dedicated.ts`
- [x] `Set.prototype.union/intersection/difference/symmetricDifference/isSubsetOf/isSupersetOf/isDisjointFrom` — `302_set_ops_dedicated.ts`
- [x] `Array.fromAsync` — `303_array_fromasync.ts`
- [x] `Promise.try(fn)` — `304_promise_try.ts`
- [x] `Iterator.from(iter)` + iterator helpers `.map .filter .take .drop .reduce .toArray` — `305_iterator_helpers.ts`
- [x] `Iterator.prototype.toArray` (already consumes the iterator) — `306_iterator_toarray.ts`

═══════════════════════════════════════════════════════════
## Weak collections / GC

- [x] `WeakRef` — `307_weakref_shape.ts`
- [x] `FinalizationRegistry` — `308_finalization_registry_shape.ts`

═══════════════════════════════════════════════════════════
## Useful misc

- [x] `console.assert(cond, msg)` — `309_console_assert.ts`
- [x] `console.group` / `groupEnd` — `310_console_group.ts`
- [x] `console.table` with an array of objects — `311_console_table.ts`
- [x] `console.dir(obj, { depth })` — `312_console_dir.ts`
- [x] Tagged template raw strings — `313_tagged_template_raw.ts`
- [x] `import.meta` — `314_import_meta.ts`
- [x] Hashbang `#!/usr/bin/env node` on the first line — `315_hashbang.ts`
- [x] `structuredClone()` with Map/Set/Date/RegExp + checking `!==` identity — `316_structuredclone_mixed.ts`

═══════════════════════════════════════════════════════════
## Subtle numerics

- [x] BigInt literals `10n`, `0xFFn`, BigInt-vs-Number comparison (`1n == 1`, `1n === 1` false) — `317_bigint_literals.ts`
- [x] Numeric separators `1_000_000` in decimal/hex/binary/octal — `318_numeric_separators.ts`
- [x] `**` operator (power) — vs `Math.pow`, with negatives, with BigInt — `319_exponentiation.ts`

═══════════════════════════════════════════════════════════
## Future suggestions (no priority)

Areas that could become fixtures when some bug shows up:

- Tail call optimization in deep recursion (RTS has TCO)
- Tagged unions / discriminated unions with switch+typeof
- Decorators (Stage 3) — `@decorator class`
- `using` declarations (ES2024 resource management)
- `await using` (ES2024)
- `Atomics.waitAsync` (ES2024)
- `String.prototype.isWellFormed` / `toWellFormed` (ES2024) — already covered but could be expanded
- Regex `v` flag (Unicode sets, ES2024)
- Named regex backreferences `\k<name>`
- Regex lookbehind `(?<=)` / `(?<!)`
- `Atomics.notify` / wait with SharedArrayBuffer (cross-thread)
- `Worker` thread API
- `Performance.now()` monotonicity
- `MessageChannel` ping-pong
- `Intl.RelativeTimeFormat`, `Intl.PluralRules`, `Intl.ListFormat`, `Intl.DisplayNames`
- `Intl.Locale` object API
- Complete object property descriptors (`writable/enumerable/configurable` combos)
- Less common Proxy traps (`isExtensible`, `preventExtensions`, `getPrototypeOf`)
- Reflect.ownKeys order
- `Function.prototype.toString` format (may give bun_node_diverge)
- Sloppy mode (non-strict) features: with statement, bizarre hoisting

═══════════════════════════════════════════════════════════
## Backlog of known RTS bugs without a fixture

Bugs already discovered via probing that haven't become cross-runtime fixtures yet.
When the fix lands, create a fixture to prevent regression:

- [ ] `Number(null)` → RTS NaN, JS 0
- [ ] `parseInt("abc")` → RTS i64::MIN, JS NaN
- [ ] `[null,1,null].join("/")` → RTS "0/1/0", JS "/1/"
- [ ] `let p: number = 8080; "" + p` → RTS "8176" (fcvt bug)
- [ ] Default param referencing another param (#640) — `function f(a, b = a)`
- [ ] Mutable closures via captured `let` (#195)
