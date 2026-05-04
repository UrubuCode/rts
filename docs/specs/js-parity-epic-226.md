# Epic #226 — Paridade JS/TS

Status em 2026-05-03. Suite TS: **457/464** (98.5%).

## Lote concluido (PRs #483-#547)

Aproximadamente 60 APIs JS adicionadas/corrigidas, ~10 issues
filhas fechadas (#208, #210, #220, #221, #371-#375, #434).
Categorias:

### Array
indexOf/lastIndexOf/includes com fromIndex; reverse; shift/unshift;
slice; concat; fill; flat; flatMap; splice; findLast/findLastIndex;
reduceRight; copyWithin; sort com deteccao de strings; values/keys/
entries; toSorted/toReversed/toSpliced/with; Array.from(length) e
Array.from(arr).

### Object
entries; assign; freeze; fromEntries; seal; isFrozen; isSealed;
getPrototypeOf; defineProperty. keys/values/entries excluem
`__proto__`.

### Math
sign; hypot; expm1; log1p; fround; sinh/cosh/tanh/asinh/acosh/
atanh; imul; clz32. Constantes SQRT2/SQRT1_2/LN2/LN10/LOG2E/
LOG10E. Aliases abs/min/max/log/random.

### String
split com limit; startsWith/endsWith com offset; match/search/
matchAll via backend regex.

### Symbol
Novo namespace `globals/symbol/`. Symbol(desc), Symbol.for(key),
keyFor, description, toString. Well-known: iterator,
asyncIterator, hasInstance, toPrimitive, toStringTag.

### URL
URLSearchParams completo (get/set/has/delete/append/getAll/keys/
values/entries/toString) + URL.searchParams getter.

### Date
setFullYear/Month/Date/Hours/Minutes/Seconds + variantes UTC;
getTimezoneOffset; toUTCString/toDateString/toJSON/toLocaleString/
toTimeString.

### Outros
TextEncoder/TextDecoder (fix critico de OOM 2.4TB em PR #485 —
GlobalClassSpec apontava para fns sem self param);
encodeURIComponent/decodeURIComponent; WeakMap/WeakSet (semantica
strong por enquanto, #217 rastreia weak real); Boolean class
(toString/valueOf); parseInt(s, radix) com semantica JS-spec.

### Destructuring (#210)
Array/objeto, defaults, rest, aninhado, em params de fn/arrow,
em for-of, em catch, alias `{a: b}`.

## Bugs descobertos e corrigidos no caminho

- Verifier error block6 invalid reference em optional chain `?.()`
  (#481): obj de Member.obj=OptChain materializado em local var temp.
- Boolean(NaN) retornando true: trocado FloatCC::NotEqual
  (Unordered) por OrderedNotEqual.
- Filter/find/etc retornando lixo: caia em string_builtin antes de
  array_builtin — corrigido com `local_array_vars` em FnCtx.
- Object.keys retornando `__proto__`: explicit skip em map iteration.

## Issues abertas pesadas (fora de escopo de PR pequena)

| # | Tema | Bloqueio |
|---|---|---|
| #195 | Mutable closures | Env-record refactor; depende de #90 |
| #207 | Event loop async/await real | Promise refactor |
| #213 | Module exports | Resolver refactor |
| #216 | Symbol como chave computada | Side-channel HashMap por objeto |
| #217 | WeakMap/WeakSet weak real + FinalizationRegistry | GC refactor |
| #218 | Proxy | Codegen interception |
| #222 | Map/Set Symbol.iterator real | Hoje so' stub |
| #223 | Dynamic import | Module resolver async |
| #211/#219/#225 | Generators / BigInt / Intl | Candidate-discard |

## Restantes na suite (7 fails)

Convertidos em issues filhas durante o epic — ver tracker.
