# PASSO 2 — `rust/natives.rs`: Coerção de Tipos Mistos

## Objetivo

Implementar extensões C nativas para resolução de operações que envolvem tipos mistos
(ex: `1 + "1"`, `"5" * 2`). O HIR injeta chamadas a `rts.natives.*` quando detecta
operandos de tipos incompatíveis.

O namespace é `rust.natives` — separado do `rts` base para isolar a lógica de coerção.

## Por que separar de `rts`?

- `rts` contém apenas primitivas tipadas puras (i64 + i64, f64 * f64)
- `natives` contém lógica de coerção que envolve múltiplos tipos
- Separação permite desativar/substituir coerção sem tocar no core de memória/escopo

## Arquivo a criar

```
src/namespaces/rust/natives.rs
```

## Primitivas

### Coerção de valor
- `rts.natives.to_string(value: u64) -> u64` — converte qualquer valor para handle de string
- `rts.natives.to_number(value: u64) -> f64` — converte para número (segue semântica JS)
- `rts.natives.to_bool(value: u64) -> bool` — converte para bool (truthy/falsy JS)

### Operações mistas
- `rts.natives.merge(a: u64, b: u64) -> u64` — merge genérico (tipo determinado em runtime)
- `rts.natives.add_mixed(a: u64, b: u64) -> u64` — `+` com coerção (número ou string concat)
- `rts.natives.eq_loose(a: u64, b: u64) -> bool` — `==` com coerção JS (não `===`)
- `rts.natives.compare(a: u64, b: u64) -> i64` — `<`, `>` com coerção JS (-1, 0, 1)

## Fluxo do HIR

```ts
// source
const valor = 1 + "1";

// após HIR — injeta natives
import { natives } from "rts";
const valor = natives.add_mixed(1, "1");
```

## Estado

Sem estado — operações puras de coerção.

## Registro

Adicionar ao `mod.rs` do namespace rust:
- `natives::MEMBERS` → MEMBERS array
- `natives::dispatch()` → chain de dispatch
