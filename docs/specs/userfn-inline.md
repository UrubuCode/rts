# Inline de user-fn no call-site (RTS_INLINE_AST) + stack-guard barato

> Plano decidido por painel de design. Objetivo: eliminar o overhead de chamada
> de função pequena no call-site quente. Medido: `pure(x)` chamada 25M× custa
> 3494ms; o mesmo cálculo inline custa 189ms (18×). No Node call==inline (V8
> inlina). Quando o RTS inlina, GANHA do Node (189 vs 292ms).

## Causa
- O inline só existe no MIR (`rts-mir/passes/inline.rs`). O top-level
  `__RTS_MAIN` (onde o loop do bench vive) é compilado 100% via AST
  (`main_fn.rs`→`lower_stmt`) → o inline MIR nunca o toca.
- `lower_user_call` (`calls/mod.rs:4295`) emite `call`/`return_call` sem inline.
- Cada call não-tail emite 2 externs (`STACK_PUSH`+`STACK_POP`) + diamante de 4
  blocos com phi (`calls/mod.rs:4463-4513`) — metade do custo.

## Fase 0 — stack-guard barato (separável, risco ~zero)
Substituir os 2 externs `__RTS_FN_RT_STACK_PUSH`/`POP` (`calls/mod.rs:4464/4494`)
por um contador thread-local lido/escrito inline em Cranelift (global data symbol
+ iadd_imm/store), mantendo o `brif` de overflow. Elimina 2 calls extern por
user-call. Ganha em TODAS as chamadas. Medir callonly.ts antes/depois (calibra o
ROI da Fase 1).

## Fase 1 — inline no call-site
4 toques:
1. `ctx.rs`: `struct InlineCandidate { params, ret, body: Vec<Statement>, cost }`
   + `struct InlineFrame { result_var, ret_ty, join_block }` + campos
   `inline_bodies: &HashMap<String,InlineCandidate>`, `inline_stack`,
   `inline_depth` no FnCtx + param em `FnCtx::new`.
2. `program.rs:419`: coleta `inline_bodies` via `inline_eligible(fn_decl)`,
   repassa a `compile_main` e `compile_user_fn`.
3. `calls/mod.rs:4461` (após `values` montado, após o tail-call path): hook
   `try_inline_user_call(ctx, cand, values)` — cria result_var + join_block,
   push_scope, bind params via declare_local(name, ty, Value), push InlineFrame,
   loop lower_stmt sobre o corpo clonado, pop, jump join, seal.
4. `control.rs:383` (topo de lower_return_stmt, ANTES do finally_stack):
   intercepta return quando `inline_stack` não-vazio → def_var(result_var) +
   jump(join_block) + return Ok(true).

Gate: `RTS_INLINE_AST` (default ON; =0 desliga, kill-switch p/ bisseção).

## Elegibilidade EXATA (conservadora v1)
Fn candidata SÓ se TODAS valem:
1. não-async.
2. sem `this` param e sem ThisExpr no corpo.
3. aridade ≤6; sem variadic/default.
4. params e ret TODOS numéricos (I8..I64/U8../Bool/F64). Sem Handle/string na v1.
5. corpo = "single-return + prelude de `const` puros": zero+ `const NAME=init`
   (binding simples) seguidos de EXATAMENTE 1 `return Some(expr)`.
6. sem try/catch/throw, break/continue LABEL, yield/await, new, member em this,
   fn/arrow aninhada com captura, arguments.
7. não-recursiva direta (corpo não chama a si mesma por nome).
8. cost(body) ≤ INLINE_AST_BUDGET (=12 nós).
MAX_INLINE_DEPTH=3 (recursão transitiva), MAX_INLINE_ARITY=6.
Args 1× garantido (values já lowered 1×). Não-elegível → call normal (0 regressão).

## Medição
bench/callonly.ts (`pure(x)=return (x*16807)%N`, 25M×):
- baseline ~3494ms → Fase 0 (?) → Fase 1 alvo ~190ms.
- `RTS_INLINE_AST=0` deve voltar ao baseline (prova que o ganho é o inline).

## Resultado medido (Fase 1, 2026-06-14)

| | Tempo | acc |
|---|---|---|
| RTS OFF (RTS_INLINE_AST=0) | 3578 ms | 75019643 |
| **RTS ON (inline, default)** | **190 ms** | 75019643 |
| Node 22 | 296 ms | 75019643 |

**18.8× mais rápido — e o RTS GANHA do Node** (190 vs 296ms). Quando inlina, o
codegen rápido do RTS vence o V8. Suite 1726/1726 ON e OFF. Race #1556 = 4000000
(inline não corrompe async). Fase 0 (stack-guard barato) não foi necessária — o
inline elimina a call inteira (logo o stack-guard junto) no caso do bench.

**Bugs corrigidos durante a implementação:** (1) elegibilidade recuada para exigir
anotação ESCALAR pura em todo param+ret (o ValTy da ABI colapsa tipos lógicos em
I64; sem isso, fns com param fn/Map/`arguments` geravam lixo/ACCESS_VIOLATION);
(2) `try_lower_if_else_return_to_select` (control.rs) emitia `return_` cru — agora
roteia pelo inline frame; (3) tracking de `terminated` no loop de stmts +
guarda dupla `!terminated && !is_unreachable()` no jump de fall-through (panic do
verifier Cranelift "terminator before end of block").

## Não-regressão (honesty floor)
- Suite 1724/1724 (ON e OFF). Monte Carlo (toFloat() inlina — single-return f64).
- Smoke `rts run` E suite (gap de cobertura JIT-symbol).
- Bateria control-flow OBRIGATÓRIA: early-return único; if/else ambos retornam
  (bloco unreachable — valida is_unreachable() antes do jump + seal do join);
  return em if sem else; callee sem return (default); inline aninhado (depth);
  callee em 2+ call-sites.
- Fixture float: fn f64-ret inlinada vs RTS_INLINE_AST=0 byte-idêntica.

## Riscos
- Selagem do join_block: selar SÓ no fim após todos os jumps; is_unreachable()
  antes do jump de fall-through (Cranelift panica se selar cedo).
- Race async #1556: inline preserva ordem de loads/stores → não cria race. Sem
  teste inline×async = risco; RTS_INLINE_AST=0 é kill-switch.
- Code bloat: budget 12 + depth 3. Medir tempo de build.
Regra de parada: se control-flow ou float não passar, recuar elegibilidade
(v1 ainda mais restrita: single-return sem if interno cobre pure/toFloat).
