# gc-arena — Documentação e Guia de Integração RTS

> Baseado na análise do repositório `kyren/gc-arena` (commit HEAD, abril 2026)
> e do módulo atual `src/namespaces/gc/` do RTS.

---

## O que é gc-arena

`gc-arena` é um crate Rust de GC incremental, exato (não conservador), com
detecção de ciclos, operando sobre **arenas isoladas**. O design central usa
**generatividade** (lifetimes brandados `'gc`) para garantir que ponteiros GC
não escapem de sua arena nem sejam contrabandeados entre arenas.

Diferença crítica em relação ao GC atual do RTS:

| Aspecto | RTS atual (`collector.rs`) | gc-arena |
|---|---|---|
| Acionamento | Explícito, usuário chama `gc.collect()` | Automático por dívida de alocação |
| Tipo de ponteiro | `u64` opaco | `Gc<'gc, T>` com lifetime brandado |
| Segurança de tipo | Runtime (decode + validação de geração) | Compilador (lifetime + trait bound) |
| Rastreamento | Conservador (trata i64 como possível handle) | Exato (derive `Collect` em cada tipo) |
| Incrementalidade | Não (sweep full ao disparar) | Sim (paga dívida aos poucos por `collect_debt`) |
| Ponteiros fracos | Não | Sim (`GcWeak<'gc, T>`) |
| Write barriers | Não (marking conservador compensa) | Explícito via `Gc::write()` / `RefLock` |
| Multithread | Shards com Mutex por shard | Single-threaded por design de arena |

---

## Módulos e Tipos Principais

### `Arena<R>` — Contêiner principal

```rust
// R: for<'a> Rootable<'a>  (root deve ser Collect para qualquer lifetime)
let arena = Arena::<Rootable![MeuRoot<'_>]>::new(|mc| MeuRoot::new(mc));

// Mutação: ponteiros GC só vivem dentro do callback
arena.mutate(|mc, root| {
    let ptr = Gc::new(mc, MinhaStruct { ... });
    root.insert(mc, ptr);
});

// Coleta incremental (paga dívida acumulada)
arena.collect_debt();

// Ciclo completo (mark+sweep de uma vez)
arena.finish_cycle();
```

**Fases de coleta** (`CollectionPhase`):
- `Sleeping` — abaixo do limiar de dívida, não coleta
- `Marking` — marcando objetos atingíveis a partir do root
- `Marked` — marking completo, pronto para finalização
- `Sweeping` — liberando objetos não-marcados

### `Gc<'gc, T>` — Ponteiro GC

- `Copy`, tamanho de ponteiro, sem overhead em mutação
- Implementa `Deref`, `AsRef`, `Borrow`
- `'gc` é invariante: impede escape de callbacks de mutação
- Criação: `Gc::new(mc, valor)` ou `Gc::new_static(mc, valor_static)`

```rust
// Referência longa-vivida (válida enquanto arena viver e valor não for coletado)
let r: &'gc T = Gc::as_ref(ptr);

// Downgrade para fraco
let weak: GcWeak<'gc, T> = Gc::downgrade(ptr);
```

**Restrição crítica**: tipos `T` em `Gc<'gc, T>` **não podem implementar `Drop`**.
A trait `__MustNotImplDrop` causa erro de compilação se `Drop` for implementado.
Limpeza de recursos deve ir em `Collect::trace` ou em finalização separada.

### `Collect<'gc>` — Trait de rastreamento

Todo tipo dentro de uma arena deve implementar `Collect`. O derive macro cuida
de structs simples:

```rust
#[derive(Collect)]
#[collect(no_drop)]   // obrigatório: GC cuida do drop, não Rust
struct MeuNo<'gc> {
    proximo: Option<Gc<'gc, RefLock<MeuNo<'gc>>>>,
    valor: i64,           // primitivos: NEEDS_TRACE = false (sem overhead)
}
```

Implementação manual (para tipos externos):

```rust
unsafe impl<'gc> Collect<'gc> for MinhaStruct<'gc> {
    fn trace<T: Trace<'gc>>(&self, cc: &mut T) {
        cc.trace(&self.filho_gc);   // rastrear cada Gc/GcWeak interno
        // primitivos (i64, f64, String, etc.) não precisam ser traced
    }
}
```

**Tipos da stdlib que já implementam `Collect`** (sem rastreamento — `NEEDS_TRACE = false`):
`bool`, `char`, `i8..i64`, `u8..u64`, `usize`, `f32`, `f64`, `String`, `str`,
`Vec<T>` (quando `T: NEEDS_TRACE = false`), `Option<T>`, `Box<T>`, `Arc<T>`,
`HashMap<K,V>`, `BTreeMap<K,V>`, tuples, arrays, `Path`, `PathBuf`, etc.

### `Mutation<'gc>` — Contexto de alocação

Passado para callbacks de `mutate`/`mutate_root`. Necessário para:
- `Gc::new(mc, val)` — alocar novo objeto GC
- `gc_ref_lock.borrow_mut(mc)` — obter referência mutável (aciona write barrier)
- `Gc::write(mc, gc)` — write barrier explícito

### `Finalization<'gc>` — Contexto de finalização

Disponível durante `MarkedArena::finalize()`. Permite:
- `Gc::is_dead(fc, ptr)` — checar se ptr está para ser coletado
- `Gc::resurrect(fc, ptr)` — ressuscitar objeto (prevenir coleta neste ciclo)
- `GcWeak::is_dead(self)` — checar ponteiro fraco durante finalização

### `Lock<T>` e `RefLock<T>` — Mutabilidade interna GC-aware

`Lock<T>` — para tipos `Copy` (equivalente a `Cell<T>`):
```rust
let lock: Gc<'gc, Lock<i64>> = Gc::new(mc, Lock::new(42));
lock.get()          // leitura (sem barreira)
// modificação requer write barrier via Gc::write
```

`RefLock<T>` — equivalente a `RefCell<T>` com write barrier automático:
```rust
let node: Gc<'gc, RefLock<MeuNo<'gc>>> = Gc::new(mc, RefLock::new(MeuNo { ... }));
node.borrow()               // leitura imutável (sem barreira)
node.borrow_mut(mc)         // leitura mutável + write barrier automático
```

`OnceLock<T>` — equivalente a `OnceCell<T>`:
```rust
once.set(mc, valor)         // inicializa uma vez, write barrier incluído
once.get_or_init(mc, || valor)
```

### `DynamicRootSet<'gc>` — Roots em runtime

Permite armazenar `Gc<'gc, T>` como `'static` fora de callbacks de mutação:

```rust
// Dentro de mutate
let root_set = DynamicRootSet::new(mc);
let dyn_root: DynamicRoot<Rootable![MeuTipo<'_>]> = root_set.stash(mc, ptr);

// Fora de mutate (pode ser guardado em estado global)
arena.mutate(|mc, root| {
    let ptr = root_set.fetch(&dyn_root);  // recupera o Gc<'gc, T>
});
```

Falha com erro em runtime se `dyn_root` pertence a arena diferente.

### `GcWeak<'gc, T>` — Ponteiros fracos

```rust
let weak = Gc::downgrade(ptr);
// Durante mutação:
let opt: Option<Gc<'gc, T>> = weak.upgrade(mc);  // None se coletado

// Durante finalização:
if weak.is_dead(fc) { /* não foi ressuscitado */ }
if let Some(strong) = weak.resurrect(fc) { /* ressuscitar */ }
```

### `Metrics` e `Pacing` — Controle de coleta incremental

```rust
let pacing = Pacing {
    sleep_factor: 0.5,    // tempo dormindo vs. alocando (padrão: 0.5)
    min_sleep: 4096,      // bytes mínimos antes de iniciar coleta
    mark_factor: 0.1,     // trabalho de marking por byte marcado
    trace_factor: 0.4,    // trabalho de trace por byte rastreado
    keep_factor: 0.05,    // trabalho por byte mantido
    drop_factor: 0.2,     // trabalho por byte dropado
    free_factor: 0.3,     // trabalho por byte liberado
};

arena.metrics().set_pacing(pacing);

// Coleta incremental: paga a dívida atual
arena.collect_debt();

// Estatísticas
arena.metrics().total_gc_allocation()     // bytes alocados no GC
arena.metrics().allocation_debt()          // dívida atual (bytes a coletar)
arena.metrics().total_gc_count()          // número de ciclos completos
```

**Fórmulas de equilíbrio** (todos os valores devem ser < 1.0):
- Objetos mantidos: `mark_factor + trace_factor + keep_factor < 1.0`
- Objetos dropados: `drop_factor + free_factor < 1.0`

`Pacing::STOP_THE_WORLD` — todos fatores 0.0, coleta apenas em `finish_cycle()`.

### `Static<T>` — Wrapper para tipos `'static`

```rust
// T: 'static pode ser usado como root sem implementar Collect
let arena = Arena::<Rootable![Static<MeuEstado>]>::new(|mc| {
    Static(MeuEstado::default())
});
```

### Write barrier — `Write<T>` e macros `field!`/`unlock!`

Quando um `Gc` precisa adotar um filho que pode não ter sido visto pelo marcador:

```rust
let write_ref: &Write<MeuTipo> = Gc::write(mc, ptr);
// Acesso a campo via macro (projeção segura)
field!(write_ref, MeuTipo, campo_filho).set(novo_filho);
```

Para tipos com `RefLock`, o write barrier é automático via `borrow_mut(mc)`.

---

## Exemplo Completo: Lista Encadeada com Ciclos

```rust
use gc_arena::{Arena, Collect, Gc, Rootable, rootable};
use gc_arena::lock::RefLock;

#[derive(Collect)]
#[collect(no_drop)]
struct Node<'gc> {
    next: Option<Gc<'gc, RefLock<Node<'gc>>>>,
    value: i64,
}

type NodePtr<'gc> = Gc<'gc, RefLock<Node<'gc>>>;

rootable! {
    struct Root<'gc> {
        head: Option<NodePtr<'gc>>,
    }
}

unsafe impl<'gc> gc_arena::Collect<'gc> for Root<'gc> {
    fn trace<T: gc_arena::Trace<'gc>>(&self, cc: &mut T) {
        cc.trace(&self.head);
    }
}

fn main() {
    let mut arena = Arena::<Root<'_>>::new(|mc| Root { head: None });

    arena.mutate_root(|mc, root| {
        let a = Gc::new(mc, RefLock::new(Node { next: None, value: 1 }));
        let b = Gc::new(mc, RefLock::new(Node { next: Some(a), value: 2 }));
        // Ciclo: a -> b -> a
        a.borrow_mut(mc).next = Some(b);
        root.head = Some(a);
    });

    arena.collect_debt();   // coleta incremental
    arena.finish_cycle();   // forçar ciclo completo
}
```

---

## Limitações Conhecidas do gc-arena

1. **Single-threaded por design**: `Gc<'gc, T>` não é `Send`/`Sync`.
   Múltiplas threads precisam de múltiplas arenas ou wrapper `Arc<Mutex<Arena>>`.
   
2. **Sem Drop em tipos GC'd**: tipos com `Drop` não podem entrar em `Gc`.
   Recursos (sockets, files, processos) precisam de wrapper que não implementa `Drop`.

3. **Write barriers explícitos**: qualquer mutação de `Gc` interno a outro `Gc`
   exige write barrier. Esquecer = corrupção silenciosa durante marking incremental.

4. **Generatividade**: ponteiros não escapam de callbacks. Código que precisa
   de ponteiros long-lived precisa usar `DynamicRootSet` + `DynamicRoot`.

5. **Rastreamento exato**: cada tipo interno a uma arena deve derivar ou
   implementar `Collect` corretamente. Um campo `Gc` não rastreado = use-after-free.

---

## Plano de Integração no RTS (`src/namespaces/gc/`)

Ver issue correspondente para lista completa de mudanças necessárias.

### Estratégia geral

O RTS usa `u64` handles como ponte ABI (extern "C"). gc-arena usa `Gc<'gc, T>`
com lifetimes brandados. A integração não substitui os handles ABI — ela
**substitui a implementação interna** do `HandleTable` e do `collector.rs`,
mantendo o contrato ABI (`u64` → `u64`) intacto para o codegen Cranelift.

```
Codegen Cranelift
    │  u64 handles  (ABI inalterado)
    ▼
src/namespaces/gc/handles.rs
    │  tradução handle ↔ Gc<'gc, Entry>
    ▼
gc-arena Arena<Root>  ← substitui HandleTable shards + collector.rs
```

### Pontos de quiescência para `collect_debt()`

Em vez de esperar o usuário chamar `gc.collect()`, o codegen pode chamar
`collect_debt()` nos pontos já definidos no CLAUDE.md:
- Retorno de funções de usuário
- Fim de métodos de classe
- Fim de escopo de closures

---

## Referências

- Repositório: https://github.com/kyren/gc-arena
- Versão no `Cargo.toml` RTS: `gc-arena = "0.5"`
- Issue de integração: ver `gh issue list --label gc-arena`
- Spec anterior: `docs/specs/INDEX.md` → `#155`
