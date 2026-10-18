# O JS de página, e as cinco peças que o fazem correr

Como um `<script>` dentro de um documento chega a executar neste motor, o que
disso vive em Rust e porquê, e o que aconteceu quando quatro dessas peças foram
apagadas sem que nada reparasse durante dezassete dias.

Data da medição: 2026-08-27.

---

## O problema, numa frase

Cada `<script>` de uma página é compilado com `new Function`, e um `new
Function` é um **programa novo**. Um programa novo não vê o topo do programa que
o criou. Portanto tudo o que dois scripts da mesma página têm de partilhar — os
globais que um publica e o outro lê, a fila de timers, o próprio ambiente de
browser — não pode viver num programa. Tem de viver **fora de qualquer
programa**, e neste motor isso quer dizer em Rust ou no objeto global.

É essa restrição, e não uma preferência de arquitetura, que decide onde cada
peça abaixo mora.

---

## As cinco peças

| peça | onde | o que é |
|---|---|---|
| `DomScope` | `rts-dom-bridge/src/scope.rs` | o saco de globais partilhado entre os `<script>` de um documento |
| `DomTimers` | `rts-dom-bridge/src/timers.rs` | a fila de `setTimeout`/`setInterval`/`rAF` da página |
| `ScriptScan` | `rts-dom/src/scriptscan/` + ponte em `rts-dom-bridge` | a varredura léxica que adapta o texto do script ao subset que o motor compila |
| `engine.run_event_loop` / `engine.take_error` | `rts-dom-bridge/src/engine.rs` | fechar o task da página, e consumir o erro que uma microtask deixou |
| a publicação no objeto global | fim de `rts-dom/src/window.ts` | o que torna o prelude alcançável de dentro de um `new Function` |

As quatro primeiras são namespaces globais declarados por `declare_global`, e
não membros de `rts:dom`. A razão é a mesma restrição: quem os chama é o prelude
da fachada, que corre sem uma importação no seu texto.

---

## O que correu mal, e como passou despercebido

O commit `46910d997` — *"delete the old engine — fifteen crates, and nothing on
the path noticed"* — apagou quinze crates do motor antigo. O `rts-dom` tinha
quatro ficheiros `.rs` (`abi.rs`, `scriptscan.rs`, `scriptscope.rs`,
`timerscope.rs`) e **cada um nomeava o motor antigo numa linha de `use`**:
`rts_engine::abi::ty::Poly`, `rts_engine::heap::handles::mark_handle`. Foram
apagados com ele, o que é a decisão certa para código que não compila mais.

A mensagem do commit estava correta sobre o Rust: o Rust compilou. O que ela não
podia saber é que o prelude `.ts` continuava a chamar `DomScope`, `DomTimers` e
`ScriptScan`, e **nada em Rust compila o `.ts`**. A suite unitária do `rts-dom`
ficou verde nos seus 718 testes, porque testa layout e estilo em Rust puro e
nunca passa por aqui.

O sintoma era um `ReferenceError` em seis das dez fixtures `claude-dom-*`, e a
partir de dentro de `__runScriptAt` nem isso: aquele caminho tem um `try/catch`
que isola um script quebrado — comportamento de browser, deliberado — e o
`catch` engolia o `ReferenceError` do prelude como se fosse a página que estava
errada. O script simplesmente não tinha efeito, sem uma palavra.

**A lição não é sobre o commit.** É que uma suite Rust verde não diz nada sobre
uma fronteira cujo outro lado é texto. As fixtures que a cobriam existiam e
falhavam; o que faltava era alguém a corrê-las.

---

## O que sobreviveu do desenho antigo, e o que não

### Continua a valer: global, e não `thread_local`

O `scriptscope.rs` antigo trazia isto escrito com o caso que o produziu: a trap
`set` do Proxy corre num contexto de execução próprio, e com um mapa por thread
a escrita caía num sítio que o leitor de fora nunca via — `count` respondia `0`
imediatamente a seguir a um `set` bem-sucedido. Foi assim que o `__d` da Meta, o
registador de módulos, desapareceu entre o script que o define e os bundles que
o chamam.

Os dois módulos novos usam um `Mutex` global pela mesma razão. O `timerscope.rs`
antigo usava `thread_local`; passou a global também, porque um `Mutex` global
responde certo em todos os casos em que aquele respondia, e também naquele em
que não.

### Deixou de ser preciso: a word tagueada

Metade da documentação do módulo antigo era sobre guardar o valor como `Poly`:
com `f64` o round-trip devolvia `undefined` e uma função voltava não-chamável.
No motor novo um nativo já recebe e devolve `u64` opaco, então não há coerção
nenhuma a evitar. O problema desapareceu com a fronteira que o causava.

### Mudou de forma: o GC

O módulo antigo registava uma função `mark_scriptscope_roots` em `rts-runtime`,
com o aviso de que marcar aloca e pode re-disparar a coleção — por isso as
palavras eram recolhidas sob o lock e o lock largado antes de marcar. Isso
existia porque um `HashMap` em memória do Rust é invisível ao mark, que é
exatamente o use-after-free do #2069.

O motor novo já tem a resposta, genérica: `entry::hold_current` /
`held_current` / `release_current`. Foi construída para os `napi_ref` e a sua
documentação descreve este caso literalmente — *um nativo que guarda um valor
depois de a chamada retornar não é um frame*. **Não se escreve marcação nova.**

Mas há um desalinhamento que muda o desenho, e vale lê-lo antes de copiar o
padrão para outro sítio.

---

## Porque os valores estão num objeto, e não num mapa

A documentação de `external` diz que *quem guarda um destes guarda um punhado*,
e `release` faz uma varredura linear da lista.

O saco de globais de uma página **não é um punhado**. Um bundle grande publica
centenas de nomes e reatribui-os em laços. Um hold por valor — que é o que o
desenho antigo fazia — poria duas varreduras lineares no caminho mais quente do
JavaScript de página.

Então:

- os **valores** vivem num objeto do runtime, um por documento, escritos com
  `set_indexed` — pelo caminho de propriedade normal, com shape e inline cache;
- o que o mantém vivo é **um único `hold_current`** por documento, sobre o
  objeto; o coletor traça as propriedades sozinho, porque é um objeto como
  qualquer outro;
- a **ordem** dos nomes (o que `count`/`nameAt` enumeram, e que um objeto não
  oferece a este crate) é um `Vec<String>` em Rust — texto, sem nada que o
  coletor precise de ver.

Nos **timers** o `external` por callback é apropriado e é o que se faz: os
timers vivos de uma página são mesmo o punhado que a doc descreve.

---

## A quinta peça: porque o prelude precisa de se publicar

As quatro peças acima repostas, os scripts ainda não corriam — e desta vez não
por código apagado.

O prelude da fachada é **concatenado ao fonte do utilizador**
(`rts-host/src/run.rs`), o que faz de `__winFor` e `__globalsFor` funções de
topo do programa do utilizador. O prologue que `__runScriptAt` injeta em cada
`<script>` chama as duas — de dentro de um `new Function`, que é um programa
novo e não as vê. Medido: `typeof __winFor` responde `"undefined"` lá dentro,
enquanto `DomScope` e `ScriptScan`, que são globais declarados em Rust,
respondem `"object"`.

A cadeia de escopo de um programa novo termina no objeto global. Publicar lá
resolve, e é o que o fim de `window.ts` faz — para `__winFor`, `__globalsFor`,
`Node` e `MutationObserver`, e só para esses: publicar o prelude inteiro poria
nomes internos ao alcance do JavaScript da página, que é superfície que ninguém
pediu.

**No fim de `window.ts` e não de `dom.ts`**, porque `DOM_TS` concatena
`dom.ts` + `scriptscope.ts` + `window.ts` e uma `class` não é içada: publicar
`MutationObserver` a partir do `dom.ts` lia-a antes de existir e derrubava o
prelude inteiro num TDZ.

---

## Uma divergência corrigida de passagem

`__runScriptAt` detetava os nomes sobre o texto **normalizado** e compilava o
texto **original**. A documentação de `scriptscan::normalize` diz exatamente
porque é que isso não pode ser — *é o texto que vai ser compilado E aquele sobre
o qual os nomes são detetados, e os dois lados têm de ver a mesma string*.

O efeito visível era `instanceof window.C`, que só o normalizado desqualifica,
nunca chegar a ser reescrito. Agora ambos os lados veem o normalizado.

---

## O estado, medido

Fixtures `tests/claude-dom-*.test.ts`, comparadas **por ficheiro** antes e
depois, em build de debug:

| | antes | depois |
|---|---|---|
| ficheiros totalmente verdes | 4 de 10 | **10 de 10** |
| ficheiros mortos em `ReferenceError` | 6 | **0** |

A suite unitária do `rts-dom` continua em `718 passed; 0 failed; 1 ignored` — o
mesmo número de antes, o que é o esperado: nada do que mudou passa por ela. Os
três crates tocados juntos (`rts-host`, `rts-dom`, `rts-dom-bridge`) dão
`1118 passed; 0 failed`.

O teste que valida o porte é `claude-scriptscan-paridade`, e ele passa: compara
cada saída do scanner em Rust com a do oráculo `.ts` que continua no
`scriptscope.ts` exatamente para isso. As 788 linhas portadas produzem o mesmo
texto que a implementação que substituíram.

### O que continua por fazer

**`instanceof window.DOMStringMap` — resolvido corrigindo a FIXTURE, não o
motor.** A asserção pedia que uma classe ausente respondesse `false`, e o
comentário dizia que essa era "a resposta certa de um feature-detect". Medido
contra os dois runtimes, que concordam:

```
obj instanceof g.NaoExiste    → TypeError: right-hand side is not an object
obj instanceof NaoExisteLivre → ReferenceError: not defined
```

Ambos **lançam**; nenhum devolve `false`. O teste passava num browser apenas
porque lá `DOMStringMap` existe de facto — resultado certo pelo mecanismo
errado. Aqui, onde a classe não existe, exigia do motor uma resposta que nem o
Chrome dá.

A fixture passou a usar o feature-detect completo (`typeof` antes do
`instanceof`), que é o que código real escreve e funciona com ou sem a classe. O
esperado continua `"10"`. Precedente para esta forma de correção: as duas
fixtures que afirmavam um `super` que o JavaScript não tem, corrigidas em vez do
motor que as passava por implementar `super` erradamente.

**A alternativa que foi recusada** era adicionar um `DOMStringMap` vazio à
fachada. Passaria o teste e seria um nome sem nada atrás — exatamente o que a
regra "uma superfície que não pode fazer o que o seu nome significa não é
enviada" existe para impedir.

### A divergência que fica registada

`unqualify_instanceof` reescreve `x instanceof window.C` para
`x instanceof C`. Quando `C` não existe, isso troca o **TypeError** que os dois
runtimes dão pelo **ReferenceError** de um nome livre. Os dois lançam e o
`try/catch` de `__runScriptAt` isola qualquer um deles, então nenhuma página se
comporta diferente por causa disto — mas um script que apanhe o erro e olhe para
o tipo vê o errado.

---

## O que verificar ao mexer nisto

1. **Correr as fixtures TS, não só a suite Rust.** É a lição inteira deste
   documento: `cargo test -p rts-dom` responde `718 passed` com o JS de página
   completamente morto.
2. **Não segurar o lock de `scope.rs` ou `timers.rs` ao entrar no runtime.**
   Alocar pode disparar uma coleção que volta a entrar aqui. Os dois módulos
   recolhem sob o lock e largam antes de chamar.
3. **Um nome que o prelude precise de expor a um `<script>` tem de ser
   publicado no objeto global**, no fim de `window.ts`. Não há outro caminho: um
   `new Function` não vê o programa que o criou.

---

## O que substitui isto tudo: o compilador passou a saber o que é um escopo

Data da medição: 2026-08-29.

As cinco peças acima existem porque `new Function` compila um **programa novo**,
e um programa novo não vê o topo do que o criou. A resposta foi reescrever o
texto do script: qualificar cada nome livre para `__G.<nome>`, e compilar o
texto reescrito. É um resolvedor de escopo escrito **fora** do compilador, sobre
texto e sem árvore — 844 linhas em `scriptscan/` mais o oráculo `.ts` que a
paridade segura — e não converge: sombreamento, destructuring, uma expressão
regular que parece uma divisão e `typeof` de nome livre são cada um um caso que
a varredura tem de aprender.

**O motor já tinha a resposta, e o bridge estava a reimplementá-la.**
`rts_core::entry::evaluate_in_scope_with_receiver(fonte, ambiente, receiver)`
compila texto cujos nomes livres resolvem contra um **objeto de ambiente**, com
cadeia (`__rts_outer`), saltos e *nearest wins* — feito pelo `Scope::lookup`, o
mesmo que resolve todo o resto. Era usado pelo `node:vm` e por mais ninguém.

O que faltava era **uma** coisa, e foi medida antes de ser escrita:

```
L1 ler a                      => 2          leitura contra o escopo ja funcionava
E1 var b = 5                  => ctx.b undefined      nao declarava para fora
E5 function f                 => ctx.f undefined      idem
E3 c = 7                      => ctx.c undefined, mas SOBREVIVIA ao fragmento
E7 this.d = 9                 => ctx.d 9              escrever no objeto ja funcionava
```

A linha do `E3` é a que só a medição dá: uma atribuição livre criava um global
**do processo**, não do contexto — dois documentos partilhavam a variável.

### A terceira porta

`rts_codegen::emit::emit_page_program` (`crates/rts-codegen/src/emit/page.rs`).
`emit_program_with_exports` diz *"nada envolve um script"* e `emit_eval_program`
lê um escopo envolvente sem escrever nele; um `<script>` de página é o terceiro
caso — lê o que os anteriores deixaram **e** declara para os seguintes, que é o
que a ECMA-262 §16.1.7 diz de script code.

Duas decisões que a fizeram encolher:

- **transformação de árvore, não mudança no hoisting**: `var x = 1` de topo *é*
  uma criação de propriedade seguida de atribuição, então dizê-lo assim reusa o
  caminho que já existe em vez de ensinar um segundo tipo de escopo ao
  `hoist_vars`. Nada em binding, captura ou saltos muda — que é a parte deste
  crate onde um erro lê a variável de outra ativação;
- **os nomes publicados entram na CADEIA a zero saltos**, e não em
  `ctx.globals`. `GlobalGet`/`GlobalSet` nomeiam o global do processo, que é o
  `E3` acima; a cadeia nomeia o objeto do documento. E dentro de funções
  aninhadas os saltos corrigem-se sozinhos, porque o `Scope` já os conta.

`crates/rts-host/src/run.rs` passou a dizer os três casos por nome — o enum
`Scoped { Nothing, Eval, Page }` — porque o mesmo texto é legal como fragmento
de `eval` **e** como `<script>`, e só a porta por onde entrou diz qual regra
vale. Um `Option` fazia deles um caso só.

### Medido

`tests/claude-page-scope-declara.test.ts`, escrita **antes** da mudança e a
asserir valores:

| | antes | depois |
|---|---|---|
| linhas certas | 3 de 8 | **8 de 8** |

As três certas de partida são controlos que a mudança não podia partir
(`let`/`const` não vazam; um segundo documento não vê o `var` do primeiro), e o
vazamento entre documentos — que **já existia** — fechou junto.

`cargo test -p rts-codegen`: 146 + 129 passados, 0 falhados.
`cargo test -p rts-host`: **412 passados**, 1 falhado —
`an_http2_request_and_response_cross_a_real_socket`, medido como pré-existente
num worktree do `HEAD` limpo (mesmo panic em `http2/delivery.rs:100`).

Uma fixture foi corrigida em vez do motor: `node_modules.rs` exigia que
`runInNewContext("({a:1})")` respondesse `undefined`. Era verdade quando foi
escrita — `runInNewContext` ia por `entry::evaluate`, que recusa o que precisa
de região — e deixou de ser quando o método passou para o caminho com receiver,
cujo objetivo declarado é precisamente que um objeto de completação atravesse.
O Node devolve o objeto. Medida como pré-existente no mesmo worktree limpo,
antes de ser tocada.

### O que falta, e é o passo que apaga código

O `rts-dom` **ainda não usa esta porta**. `__runScriptAt` continua a reescrever
o texto. Ligá-lo apaga `__bindGlobals`, `__scanImplicitGlobals`, `__scopeNames`,
`unqualify_instanceof`, o `scriptscan/` e o oráculo que a paridade segura — e o
`<script>` passa a correr com **o texto que veio na página**, o que fecha por
construção a classe de divergências que a normalização abre uma de cada vez.
