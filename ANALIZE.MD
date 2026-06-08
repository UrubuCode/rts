# Análise de Falhas — Namespaces RTS

Auditoria multi-agente (30 agentes) sobre todos os 73 diretórios de namespace
(`crates/rts-runtime/src/namespaces/` + `globals/`). Segunda passada verificou
adversarialmente cada achado high/critical.

**Resultado: 56 falhas encontradas, 37 confirmadas (high/critical).**

Data: 2026-06-08. Foco: código `extern "C"` cruzando a fronteira JIT, HandleTable
(u64 = gen:16 + slot:48), GC mark+sweep, ponteiros crus.

---

## CRÍTICO — explorável agora

### 1. Vazamento de slot de handle → DoS

Padrão: `mem::replace(e, Entry::Free)` dentro de `with_entry_mut`, depois
`free_handle()` fora do lock. `free()` vê o slot já `Free`, retorna cedo, nunca
empurra para o `free_list` nem decrementa `LIVE_HANDLES`. Slot perdido para
sempre. Chamadas repetidas atingem `HANDLES_MAX` (5M) → abort do processo.

| Arquivo | Linha | Função |
|---|---|---|
| `tls/mod.rs` | 61-91 | `client` |
| `process/mod.rs` | 85-105 | `wait` |
| `process/mod.rs` | 109-129 | `kill` |
| `thread` | — | `join`/`detach` (via `take_join_handle`) |

**Fix:** chamar `free_handle` ANTES do `mem::replace`, ou decrementar
`LIVE_HANDLES` manualmente quando marcar `Free`.

### 2. Transmute de ponteiro de função sem validação → código arbitrário / ABI quebrado

- `globals/message_channel/instance.rs:44-56` — `invoke()` limita a 3 args; caso
  default transmuta para `fn(i64,i64,i64)` e **trunca**. Callback com args ligados
  somando 4+ → corrupção de stack. **critical**
- `globals/fetch/instance.rs:230` — `PROMISE_THEN2` caminho síncrono transmuta a
  callback u64 sem checar se é Function handle ou ptr cru. Arrow reificado (params
  tipados) → valor do handle chamado como endereço de código. **critical**.
  `PROMISE_TRY` faz certo (`read_function_data`) — copiar esse padrão.
- `globals/string/search.rs:503-516` — `REPLACE_REGEX_FN` transmuta por contagem
  de grupos de captura, ignora o campo `arity` da Function. Incompatível → UB.
  **critical**
- `events/mod.rs:106,123,142,162` — `on()`/emit transmutam u64 do usuário sem
  validação.
- `globals/timers/instance.rs:181`, `collections/map.rs:786` (SET_FOR_EACH) —
  mesmo padrão.

### 3. Use-after-free em atomic

`atomic/mod.rs:18-52` — `with_atomic_{i64,bool,f64}` derefenciam o ponteiro do
`Box` DEPOIS de liberar o lock do shard. Free/GC concorrente → UAF. Comentário
`// SAFETY: heap-stable` está incorreto: "stable" só vale enquanto o slot vive.

---

## ALTO — precisa de guards de input/overflow

### 4. Cast de ponteiro negativo/inválido

Só checam `ptr==0`, faltam `ptr<0` / `len<0`. i64 negativo → endereço enorme
(complemento de dois) → OOB read/write em `from_raw_parts`.

| Arquivo | Linha | Função |
|---|---|---|
| `crypto/mod.rs` | 262 | `random_bytes` |
| `crypto/mod.rs` | 343 | `hash_update_bytes` |
| `crypto/mod.rs` | 379 | `sha256_bytes` |
| `crypto/mod.rs` | 389 | `hex_encode` |
| `crypto/mod.rs` | 418 | `base64_encode` |
| `hash/mod.rs` | 39 | `hash_bytes` (+ sem checagem de geração → UAF) |
| `fs/mod.rs` | 49 | `read_all` |
| `fs/mod.rs` | 79 | `write_bytes` |
| `globals/url/instance.rs` | 187 | `str_from_parts` (+ `from_utf8_unchecked`) |
| `globals/readable_stream/instance.rs` | 416 | `COMPRESSION_STREAM_NEW` (+ UAF GC-root) |
| `globals/intl/instance.rs` | 14 | `str_from_raw` |

**Fix:** crypto deveria usar `U64` para ponteiros como o resto do codebase
(fs/io/net). Validar `ptr > 0 && len >= 0` antes de `from_raw_parts`.

### 5. Overflow sem checagem → panic/OOM/UB

- `bigfloat/fixed.rs:26` — `from_i64`: `(v as i128) * pow10(scale)`.
  `from_i64(i64::MAX, 36)` estoura i128. `from_str` usa `checked_mul` — ser
  consistente. (`pow10` em `:324-329` também sem `checked_mul`.)
- `audio/mod.rs:88`, `asio_audio/state.rs:95` — `capacity_frames * channels`
  multiplicação sem checagem antes de `VecDeque::with_capacity`. Input i64
  ilimitado do TS → overflow/OOM.

### 6. Bugs de lógica

- `collections/map.rs:1955` — `key_h > 0xFFFF_FFFF` comparação **com sinal**;
  handles com bit 63 setado (geração alta) ficam negativos → handles válidos
  viram string em vez de deref. Quebra `Object.groupBy`/`Map.groupBy`.
  **Fix:** `(key_h as u64) > 0xFFFF_FFFF`.
- `runtime/mod.rs:59` — `eval()` deriva caminho do arquivo temp só do PID, sem
  thread id. `eval()` concorrente sobrescreve o código um do outro → thread
  executa código errado. **Fix:** incluir contador atômico/thread id no path.
- `globals/abort/instance.rs:233-260` — `SIGNAL_ANY` spawna thread que captura
  handles por até 5s; se pai dropa refs e GC roda, sinais de abort perdidos
  silenciosamente (não corrompe memória — gen-check protege — mas perde sinal).

---

## MÉDIO — notável

- `time/mod.rs:50` — `u128 as i64` em `unix_ns` (overflow ano 2262, risco real baixo).
- `fs/mod.rs:227` — `copy()` retorna `u64 as i64`, arquivos > 9 EB viram negativo
  (interpretado como erro -1).
- Mutex `lock().unwrap()` panic por poison: `globals/function/props.rs:50,72`,
  `globals/symbol/mod.rs:44,74,82,90`. Codebase usa
  `unwrap_or_else(|e| e.into_inner())` em outros lugares — ser consistente.
- `globals/blob/instance.rs:31` — `i64 as u8` truncamento silencioso em
  `concat_parts`, perda de dados.
- `globals/regexp/mod.rs:141,156` — grupos de captura indefinidos usam sentinela
  `i64::MIN + 2`; se JS tratar como handle sem validar → deref inválido.
- `buffer/mod.rs:103,120,137,154,672,676` — `.try_into().unwrap()` alcançável de
  input do usuário.
- `globals/function/ops.rs:182` — `panic!` para aridade > 16.
- `gc/generator.rs:316,354` — transmute de `fn_ptr` sem validação (ordenação
  atual segura mas frágil).
- `parallel/pool.rs:28` — `.expect()` em init do rayon (RTS_THREADS controlável).
- `globals/timers/instance.rs:221-227` — `setInterval`: race entre `register_timer`
  e `cancel_timer`; handle não liberado se thread panica.
- `bigfloat/fixed.rs:299` — cast `u128 as i128` em `i256_div` (correto na prática,
  mas armadilha sutil).

---

## BAIXO

- `globals/reflect/ops.rs:165` — `Vec::contains()` em loop = O(n²); usar `HashSet`.
- `globals/symbol/mod.rs:238,249,261` — `unwrap()` em testes.
- `globals/url/instance.rs:308` — `url_field` retorna handle de `Entry::Env` sem
  propagação de mark no GC (gen-check protege contra corrupção, mas componentes
  da URL podem virar inacessíveis).

---

## Recomendação — quick wins (limpos, baixo risco)

1. **Cluster de vazamento de handle (#1)** — `tls/process/thread`. Mecânico,
   inequívoco. DoS claro.
2. **`collections/map.rs:1955` (#6)** — uma linha: cast para `u64`.
3. **`promise.then2` (#2)** — espelhar `PROMISE_TRY` (`read_function_data`).

JSON completo do run: `wijjad6lg.output` (run `wf_ff52d8b6-9d8`).
