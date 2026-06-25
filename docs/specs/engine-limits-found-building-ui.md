# Limites do motor novo encontrados construindo a UI (mapa pro futuro)

> Resultado lateral — e valioso — do experimento da stack de UI (DOM + canvas +
> componentes em TS sobre `rts:render`). Cada limite abaixo foi encontrado NA
> PRÁTICA ao escrever UI real, com o workaround que usamos e o que o motor
> precisaria implementar "futuramente". É uma lista priorizada por uso real, não
> teórica. Datado 2026-06-25, motor `rts-codegen-new`.

## Como ler

Cada item: **o que falha** · **onde bateu** · **workaround atual** · **o que o
motor precisa**. Ordenado por quanto atrapalha a ergonomia de UI.

---

### 1. `const` top-level não captura dentro de função
- **Falha:** `const SLOT = 3; function f() { usa SLOT }` → `unbound identifier`.
- **Onde:** layout-TS (slots de estilo), qualquer constante usada em helper.
- **Workaround:** literais inline, ou variáveis module-level (`let`), ou reidratar
  dentro da função.
- **Motor precisa:** capturar bindings top-level (const/let) no escopo de funções
  aninhadas. Ver `project_multi_declarator_capture_bug`.

### 2. Método sobre RETORNO de getter/call não despacha
- **Falha:** `const cv = app.canvas; cv.box(...)` ou `app.canvas.box(...)` →
  `receiver class not statically dispatchable` / shape não provada.
- **Onde:** App expondo `canvas` por getter; qualquer fachada que devolve objeto
  pra encadear.
- **Workaround:** métodos DIRETOS na instância já provada (`app.box()` em vez de
  `app.canvas.box()`); a fachada delega internamente.
- **Motor precisa:** propagar a classe de retorno de getter/método para habilitar
  dispatch no valor retornado (parte já anda p/ método-de-call via `local_classes`;
  falta getter e campo). Ver `project_new_engine_dispatch_limits`.

### 3. `boolean` retornado de método não coage em `?:`/condição
- **Falha:** `let on = obj.method(); const c = on ? a : b;` →
  `cannot coerce Tagged to Bool`.
- **Onde:** checkbox/toggle retornando `boolean`.
- **Workaround:** usar `number` (0/1) em vez de `boolean` para estado vindo de
  método; comparar com `!== 0`.
- **Motor precisa:** coerção Tagged→Bool no caminho de retorno de método (hoje o
  bool-de-método chega Tagged e o `?:` não coage).

### 4. `.length` sobre string de método/param/reassigned
- **Falha:** `function f(s: string) { s.length }` ou `const t = obj.m(); t.length`
  → `.length on a receiver of unproven shape — dynamic-length is a separate path`.
- **Onde:** textInput (medir/cortar texto digitado).
- **Workaround:** evitar `.length` sobre essas strings; só concatenar (`a + b`
  sempre funciona). Backspace/edição de texto ficam bloqueados sem isto.
- **Motor precisa:** resolver `.length` (e provavelmente `.substring`/índice) sobre
  string de shape não-provada — rotear para o caminho dinâmico de string.

### 5. Array-indexado de string + uso subsequente
- **Falha:** `const names = ["a","b"]; app.tab(..., names[i], ...)` baila em alguns
  usos (o elemento string não tem shape provada).
- **Onde:** lista de nomes de abas/itens.
- **Workaround:** literais diretos, ou arrays paralelos de primitivos.
- **Motor precisa:** shape provada para `array[i]` quando o array é de string/obj.

### 6. `performance.now()` retorna 0
- **Falha:** `performance.now()` sempre devolve `0` (delta time impossível por ele).
- **Onde:** App loop / animação.
- **Workaround:** usar `rts:time` (`time.now_ms()`, monotônico) — funciona.
- **Motor precisa:** ligar `performance.now()` ao clock real (hoje o prelude
  `performance` não tem o timeOrigin/now efetivo no caminho atual).

### 7. Input só legível DENTRO do frame
- **Não é bug — é regra de uso, documentar:** ler `input.*` ANTES de
  `beginFrame` devolve estado vazio; o `beginFrame` é que transfere os eventos do
  SO pro contexto. Ler input sempre após `beginFrame`.

---

## O que NÃO foi limite (já funciona — bom saber)

- **Classes com métodos + encadeamento** (`new C().m().m()`): OK.
- **Getter/setter de propriedade** (`el.textContent = x`): OK (é a base da fachada
  DOM real).
- **Objeto global singleton via prelude** (estilo `console`/`document`/`createApp`):
  OK.
- **Método retornando `T | null` + `=== null`/`if(x)`**: OK (desde que seja método
  de classe, não função livre).
- **Array de instâncias** (length/índice/for-of/push): OK.
- **String vinda do Rust** (handle GC) usada como string: OK.
- **Multi-window**: OK (UiCtx por handle; pump global por WindowId).
- **Recursão profunda + muitas chamadas ABI por frame** (layout): OK.

## Conclusão

Apesar de 6 limites reais, foi possível construir: DOM real (MDN), layout em TS,
render/input abstratos (backend plugável), canvas ergonômico, App loop com delta
time, e uma biblioteca de componentes (button/slider/checkbox/progressBar/panel/
tabs/textInput/layout-automático) + multi-window. A fundação é sólida; os 6 itens
acima são o roteiro de refinamento do motor para UI rica — implementar futuramente,
maior-impacto-primeiro (1, 2 e 4 destravam mais).

---

### 8. `moveWindow`/`set_outer_position` só aplica DEPOIS do loop começar
- **Não é bug — é timing do winit:** chamar `app.moveTo(x,y)` ANTES do primeiro
  `beginFrame`/`pump` não reposiciona a janela (o event loop ainda não rodou).
- **Workaround:** chamar `moveTo` DENTRO do loop, após alguns frames
  (`if (frameCount() > 2)`), uma vez.
- **Motor/backend poderia:** aplicar a posição pendente no primeiro pump, ou expor
  posição inicial no `openWindow`.
