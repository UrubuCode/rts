# Onde está o gap — medição e plano, 2026-08-28

**Como este documento foi produzido.** 46 agentes leram o código (não as docs) por
subsistema, 32 deles como verificadores adversariais tentando refutar cada achado;
19 achados sobreviveram e 13 foram refutados ou tiveram o mecanismo corrigido. Em
paralelo, 6 frentes de pesquisa em fontes primárias (v8.dev, webkit.org, papers do
Static Hermes, Hopc, weval/PLDI 2025, Static TypeScript). Depois, 4 planos
independentes com premissas opostas, 3 juízes com lentes diferentes, e uma síntese.
Tudo que é citado como número foi medido nesta máquina ou tem fonte nomeada.

---

## 0. A régua, e um erro meu nela

Medido 2026-08-28, `target/release/rts.exe` (release real, não `fast`), Node v25.9.0,
Bun 1.4.0. Tempo interno via `Date.now()` **dentro** do programa, então startup está
fora. **Min de 5, um engine por vez.**

| bench | node | bun | rts | rts/melhor |
|---|---:|---:|---:|---:|
| arith_i32 | 27 | 50 | 385 | **14,3x** |
| arith_f64 | 71 | 73 | 85 | 1,2x |
| call_direct | 79 | 17 | 61 | 3,6x |
| call_method | 8 | 14 | 180 | **22,5x** |
| call_closure | 9 | 16 | 193 | **21,4x** |
| prop_read | 20 | 43 | 255 | 12,8x — ver §0.2 |
| prop_write | 16 | 26 | 107 | 6,7x |
| prop_keyed | 9 | 14 | 70 | 7,8x |
| alloc_obj | 8 | 9 | 379 | **47,4x** |
| array_idx | 52 | 41 | 645 | **15,7x** |
| array_push | 156 | 73 | 1596 | **21,9x** |
| string_idx | 41 | 17 | 930 | **54,7x** |
| math_call | 40 | 44 | 65 | 1,6x |
| fib_rec | 18 | 14 | 70 | 5,0x |

Startup de processo vazio: rts ~60 ms, bun ~65, node ~115. **Startup não é problema.**

### 0.1 O erro, declarado

A primeira corrida desta tabela intercalou os três engines dentro do mesmo laço e
inflou o lado lento: saiu `call_method` 41,1x, `call_closure` 46,1x e `fib_rec` 9,6x.
Os números reais são 22,5x, 21,4x e 5,0x. **O painel de planos recebeu os números
inflados.** Isso não moveu nenhuma etapa — a síntese ancorou tudo em ns por operação
(24,5 ns por chamada de método, 76,38 ns por `new`) vindos de ablação, e não nas
razões da tabela — mas fica registrado porque um número que não é re-medido vira
alegação.

### 0.2 `prop_read` não é o que a linha parece, e isso muda um item do plano

O plano marcava `prop_read` como contradição não resolvida: 12,8x na tabela contra
3,3 ns numa leitura monomórfica quente. Resolvido por subtração — mesmo laço, com e
sem as duas leituras, min de 5, 10⁸ leituras:

| | laço só | com 2 leituras | delta | ns por leitura |
|---|---:|---:|---:|---:|
| node | 19 | 20 | 1 | **0,010** |
| bun | 26 | 43 | 17 | 0,170 |
| rts | 45 | 245 | 200 | **2,000** |

**O RTS lê uma propriedade em 2 ns e isso está certo.** O node lê em 0,01 ns porque
o V8 **removeu as leituras** — são invariantes do laço. A razão de 12,8x compara o
RTS fazendo 100 milhões de leituras contra o node fazendo aproximadamente nenhuma.

Consequência para o plano: `prop_read` **não pertence** ao modelo de objeto nem ao
cache. Pertence ao LICM, ou seja à Etapa 6. Nenhum trabalho em cache de propriedade
move essa linha.

### 0.3 `string_cat`: quadrático, e a razão cresce com n

Fora da tabela porque não tem razão fixa. `s += "ab"`, min de 1 (leva minutos):

| n | rts | node | bun | razão |
|---:|---:|---:|---:|---:|
| 25 000 | 267 ms | 3 | 1 | 267x |
| 50 000 | 1 040 ms | 2 | 2 | 520x |
| 100 000 | 4 713 ms | 8 | 5 | 943x |
| 200 000 | 30 535 ms | 10 | 7 | **4 362x** |

O tempo do RTS faz 3,9x → 4,5x → 6,5x a cada duplicação de n (acima de 4 por efeito
de cache); node e bun são lineares. Citar "12 365x" como se fosse uma constante está
errado: a razão é uma função de n porque um lado é O(n²). O que se afirma é a
**forma**, e o falsificador do conserto é a forma, não o valor.

### 0.4 O mid-end do Cranelift: medido, e não é o caminho

`opt_level` fica no default `none`, o que desliga o egraph inteiro. Ligando com
`RTS_CL_OPT=speed`, min de 3, nesta máquina:

| bench | off | speed | |
|---|---:|---:|---:|
| arith_i32 | 380 | 381 | 1,00x |
| arith_f64 | 85 | 86 | 0,99x |
| prop_read | 248 | 211 | 1,18x |
| fib_rec | 70 | 71 | 0,99x |
| array_idx | 652 | 631 | 1,03x |

Ruído, com uma exceção modesta. Isso **confirma** o comentário que já está em
`target/mod.rs:1065-1083`, e a causa que ele dá é a tese deste documento inteira:
*o mid-end não enxerga através de uma chamada opaca, e o IR deste motor é quase só
chamada opaca.* O mesmo comentário diz quando volta a valer — "o dia em que o runtime
deixar de ser uma chamada por operação". Ou seja: depois da Etapa 3, não antes.

Fica um item real de uma tarde, que o comentário admite não ter argumento: o caminho
AOT pede `Priority::CodeQuality` e recebe o otimizador desligado.

---

# PLANO DE REGISTRO — RTS, performance

*Síntese dos quatro planos, dos três julgamentos, e de doze arquivos deste repositório que os quatro planos não leram. Tudo que cito de código ou doc foi conferido nesta árvore hoje.*

---

### 0.5 O defeito que a tabela inteira não via: a compilação era cúbica

**Achado e corrigido em 2026-08-28, depois de o dono do projeto observar que os
benchmarks medem só o cache quente.** A observação estava certa e leva mais longe
do que ela mesma: todos os quinze programas da tabela são laços de milhões de
iterações sobre um punhado de sítios, então nenhum deles mede o que acontece
quando um programa tem MUITO CÓDIGO em vez de muitas iterações.

Um programa de `n` acessos a propriedade dentro de UMA função, medido com
`RTS_TIMING=1`:

| n | `emit` | `place` | `prepare` | total |
|---:|---:|---:|---:|---:|
| 100 | 0,7 ms | 19 ms | **68 ms** | 176 ms |
| 200 | 1,1 ms | 40 ms | **542 ms** | 667 ms |
| 400 | 2,1 ms | 98 ms | **4 578 ms** | 4 895 ms |
| 1600 | — | — | — | **197 897 ms** |

`emit` e `place` são lineares; `prepare` faz ×8 a cada duplicação de `n`. Cúbico.
Uma função de 1 600 acessos levava **3 min 18 s** para compilar e o Node roda o
arquivo inteiro em 141 ms. Dividir as mesmas 1 600 sentenças em 64 funções de 25
levava 0,37 s — o custo é por função, não por programa.

**O eixo é o número de BLOCOS, e o inline cache é o que os cria.** Contado com
`rts ir | grep -c '^block'`, por sentença:

| sentença | blocos |
|---|---:|
| `z = z + 1.0` (f64 provado) | **0** |
| `z += g(z)` | 9 |
| `z += p[i]` | 10 |
| `z += p.a` | **13** |

**A causa.** `verify/rules.rs::dominators()` era a matriz iterativa de bitmap:
`Vec<Vec<bool>>` de `blocos × blocos`, refeita até ponto-fixo, alocando um
`Vec<bool>` do tamanho do número de blocos por bloco por rodada. E o único leitor
dela, `check_cleanups`, só a lê dentro do laço sobre as regiões de cleanup — ou
seja, **um programa sem `try`/`finally` computava a matriz inteira e a jogava
fora**.

**A correção.** `verify/dominance.rs`, novo: Cooper–Harvey–Kennedy (imediato por
bloco, ponto-fixo sobre reverse postorder, `intersect` subindo as duas cadeias
pelo número de postorder), mais intervalos `[enter, leave)` da árvore de
dominadores para consulta O(1); e a computação passou a ser preguiçosa. Medido
entre dois binários que diferem apenas por isso:

| n | HEAD | com o patch | ganho | HEAD c/ `finally` | patch | ganho |
|---:|---:|---:|---:|---:|---:|---:|
| 200 | 487 ms | 120 ms | 4,1× | 1 240 ms | 155 ms | 8,0× |
| 400 | 3 224 ms | 317 ms | 10,2× | 8 818 ms | 247 ms | 35,7× |
| 800 | 25 562 ms | 302 ms | 84,6× | 66 997 ms | 467 ms | 143,5× |
| 1600 | 197 897 ms | 796 ms | **248,6×** | >420 000 ms | 1 117 ms | **>376×** |

O que importa mais que os fatores é a forma: a coluna do patch cresce ~linear
(120 → 796 para `n` ×8) onde a do HEAD fazia ×8 por duplicação.

**Portão.** 308 testes do `rts-cranelift` passam. A suíte TypeScript é
**idêntica arquivo por arquivo** entre os dois binários — 752/813, LOST 0,
ganhos 0, mudanças de modo 0 — que é o que um fix de performance deve dar.

**E uma correção de método, porque ela custou uma alegação errada.** A primeira
comparação usou um `target/release/rts.exe` que eu copiei em vez de construir. Ele
era de 05:25 e o commit `51c243a7` é de 05:28, então o binário era anterior ao
último commit de código: a comparação creditou ao patch **13 arquivos** que
passaram a passar por causa daquele commit — `reflect_api`, `proxy_phase2`,
`function_expression` e outros. Reproduziam à mão, e nenhum era meu. Contra o
baseline construído do HEAD o número correto é zero. O CLAUDE.md já manda
*construir* o binário antes da primeira edição; copiar um que já existe não é a
mesma coisa, porque um binário em `target/` não tem procedência.

---

## 1. Onde você errou

Primeiro o que está certo, porque é muito e porque a resposta seguinte só faz sentido em cima disso.

A camada de máquina está certa. `arith_f64` a 1,2x e `math_call` a 1,5x não são consolo: são a prova de que Cranelift, o regalloc, a codificação NaN-box e a convenção estão corretos. O layout está certo — endereço é `base + idx*128`, aritmética pura, leitura monomórfica quente a 3,3 ns. O mundo fechado já existe de verdade (`graph.rs` emite o programa inteiro numa compilação). A régua de corretude é real e cara de construir: 746/808 e 728/762. E há uma linha na tabela de ações que ninguém no debate mencionou: `flow throw+catch` mede **0,14x** — este motor lança e captura sete vezes mais rápido que o node. Isso é um ativo e ninguém propôs protegê-lo.

O maior ativo do repositório, porém, não é código: é `docs/codegen/plan.md §9`, onde onze ideias razoáveis estão **refutadas com evidência**. Entry tax 0,53 ns. `with_current` medido negativo duas vezes. Convenção de chamada 0,3–0,6 ns. Dimensionar célula de IC por espécie: corrupção de memória quando foi tentado. Isso vale mais que o código que economizou.

Agora a causa. Não é arquitetura, não é Cranelift, não é o modelo de objeto, e — isto importa — **não é falta de tipos**. São três coisas, e as duas primeiras são a mesma coisa vista de dois lados.

### 1.1 A regra "construa a capacidade antes do cliente" produziu oito capacidades sem cliente

Conte comigo, tudo verificado hoje:

| capacidade | estado | clientes |
|---|---|---|
| `Inst::ElementLoad` | declarada, baixada em 6 instruções | 1 produtor, inalcançável |
| `Inst::CallIndirect` | declarada, baixada | **0** |
| `Builder::alloc` / `Inst::Alloc` | declarada, baixa para `call rts_alloc` | **0** |
| `Repr::I8`, `I16`, `F32` | no lattice da máquina | **0 produtores na linguagem** |
| conversão de largura inteira | **não existe no conjunto de instruções** | — |
| `gc::describe_frames` | pronta e testada | **0** |
| `observe::CodeMap` | pronta e testada | seus próprios testes |
| `observe::PositionMap` | construída **para toda função compilada** | seus próprios testes |
| `Inst::IntArith` | declarada | 0 produtores em qualquer programa |

Nenhuma dessas está faltando. Cada uma foi construída ao padrão do crate: documentada, testada, correta. O que falta é o **fio**. E o fio não tem dono por construção: a regra 2 do `rts-cranelift` proíbe a máquina conhecer linguagem, o `rts-core` não pode conhecer código emitido, e o `rts-host` — a única crate autorizada a nomear os três — tem na própria README a regra "torne explícitos os acordos entre eles", e **este acordo nunca foi feito**. `docs/engine/the-unwired-keystone.md` já diz isso melhor que eu; foi escrito há dias e ainda não é um plano.

Isto é uma causa de *processo*, não de engenharia. A regra que produziu uma camada de máquina correta é a mesma que produziu oito peças mortas, porque ela não tem uma segunda metade dizendo quem liga.

### 1.2 Uma decisão de coletor que nunca foi precificada como decisão de compilador

`entry/roots.rs` faz varredura conservadora: reconhece referências **pelo padrão de bits**. Isso é uma decisão de runtime perfeitamente defensável e foi tomada assim.

O que ela custa está escrito em `docs/codegen/element-load.md`, e é a frase mais cara desta árvore:

> *um derivado tipado de máquina de uma referência de heap é invisível a um coletor conservador. Um endereço base, um campo sem box, um elemento estreito — cada um deixa de parecer uma referência no momento em que se torna útil, e deixa de ser raiz no mesmo instante.*

Um endereço base, um campo desempacotado, um elemento estreito, um inteiro de 32 bits: **esse é o vocabulário inteiro de um caminho rápido.** Ele é ilegal aqui, e é por isso que as nove linhas da tabela acima estão mortas — não por falta de trabalho, por uma precondição única e não ligada.

Já foi pago em dinheiro: o caminho rápido de `for`-`of` foi implementado, mediu **−15,3%** (`array for-of 16`, 55,95 → 47,41) e respondeu **errado em 53 de 60** casos. Zero errados quando o array guarda números; 53 quando guarda objetos; 57 quando guarda closures. Nenhum microbench numérico veria isso. Três dos quatro planos propostos agendam exatamente esse caminho de novo, e oferecem como portão exatamente o instrumento que dá zero.

E o mesmo bloqueio, do outro lado: `context.callees` e `context.pending_arguments` **são raízes de GC** — `roots.rs:124` os enumera nominalmente, filtrados como palavras de pilha. Os planos 1, 3 e 4 propõem apagá-los para ganhar 7,3–10,2 ns. Isso não é uma otimização de 8 ns; é um use-after-collect, a menos que alguma coisa saiba o que está rodando sem eles. É `describe_frames` de novo.

### 1.3 O instrumento nunca foi o alvo

`docs/codegen/action-table-2026-08-26.md` abre com o aviso: esta máquina estava compilando durante a sessão e `bench/analytic.ts` **pôs as noventa linhas em faixas sobrepostas**, com uma linha de baseline indo de 212 a 468 ns entre corridas dela mesma. O método que sobrevive é min-de-**cinco** dentro de um processo, alternado entre binários, declarado como par. Os quatro planos especificam min-de-3 e limiares de 5%.

Pior: o instrumento mente em seis lugares, e isso está catalogado em `plan.md §7`. `call free function` (3,21) e `call arrow` (3,11) **não contêm chamada** — o corpo é substituído. `alloc object literal 2/8` estão abaixo do piso de 1,27 ns porque são escalar-substituídos: essas linhas não medem alocação. `array map/filter 16` medem majoritariamente `closure_new` dentro do laço. Uma linha de node abaixo de ~1,2 ns é um laço que o JIT deletou, não um custo.

E a tabela de 15 linhas que fundamentou o debate inteiro **omite as piores linhas reais**: `regex exec+group` 197x, `string split 16` 137x, `regex test` 78x, `alloc Uint8Array` 62x, `array map` 47x, `array filter` 42x, `generator next` 30x, `template literal` 27x, `JSON stringify` 20x, `JSON parse` 12x. Isso é implementação de biblioteca em `rts-core` — não é protocolo, não é tipo, e não sai de graça de nenhuma etapa que qualquer um dos quatro planos propôs.

**Resumo da causa raiz, em uma frase:** você construiu um motor correto cujas capacidades rápidas estão todas atrás de uma precondição única e não ligada, e mediu o progresso contra um instrumento que em seis lugares mede outra coisa. Não é falta de tipos, e não é falta de um motor novo.

---

## 2. Veredicto sobre `rts-codegen-v2`

**Não.** E a recusa é do meio, não do fim.

Os quatro planos convergiram nisso e os argumentos deles estão certos, mas o argumento decisivo é mais estreito e mais forte do que os quatro escreveram: **um v2 não toca em nenhuma das causas.** As oito capacidades mortas estão no `rts-cranelift`; a varredura conservadora está no `rts-core`; `Str::concat` está no `rts-core`; os elementos em tabela lateral estão no `rts-core`; o fio que falta pertence ao `rts-host`. Um `rts-codegen-v2` reescreveria o emissor — a única peça cujo defeito é local e catalogado — e deixaria intactas todas as três causas.

Some-se o precedente: `rts-codegen-new` custou meses e foi deletado; a lição registrada é *"a phase is finished when the old code is gone"*. E um motor paralelo é o único formato que **não pode** ser medido per-file contra binário guardado, que é a única forma que "sem regressão" tem aqui.

**A concessão é concreta e não é simbólica.** Há duas coisas nesta árvore que merecem uma versão nova, e nenhuma é um crate:

1. `ir::Function` é append-only (`push_block` / `push_block_param` / `push_inst` / `push_const` / `set_terminator`). Nenhum passe *pode* ser escrito antes de a estrutura ganhar substituição e remoção. Ela tem 395 linhas — cabe no teto, e o crescimento é aditivo.
2. `Inst` precisa carregar **efeito** (no sentido do `Effects: None` do Static Hermes), porque é o efeito, não o opcode, que autoriza CSE/DCE/LICM. `inst.rs` tem 954 linhas contra teto de 1000 — isso **exige** virar pasta antes.

Isso é v2 da estrutura de dados do IR, dentro do `rts-cranelift`, atrás da mesma API de builder. É a Fase 6, e é condicional ao número da Fase 5.

---

## 3. Regras que valem em todas as etapas

Antes das etapas, seis regras. Cinco vêm dos juízes; a sexta vem de contar linhas nesta árvore.

**R1 — binário NOMEADO por etapa, não baseline rolante.** `target/e0.exe` + `e0.json`, `target/e1.exe` + `e1.json`, e assim por diante, além do baseline rolante que o CLAUDE.md recomenda. Motivo: os três modos de falha que este plano tem em comum (representação de string, cabeçalho de array, layout de callable) são **latentes**. Um LOST descoberto na etapa 5 contra um baseline rolante não é atribuível, e reverter a etapa 2 invalida todo número medido nas 3 e 4 porque a árvore contra a qual foram medidos deixou de existir. Custo: ~10 MB por etapa em `target/`, que já é ignorado.

**R2 — LOST grava o MODO, e mudança de modo conta.** Comparar só o conjunto que passa é cego. Um arquivo que hoje falha por asserção e passa a travar registra zero em todos os portões propostos pelos quatro planos — e 36 dos 62 arquivos que já falham são `node_fs`/`node_dns`/`node_tls`/`node_dgram`/`net`/`tls`, ou seja a família onde um bug de raiz aparece como timeout. O relatório grava `passou | asserção | exceção | timeout | crash` por arquivo; uma mudança de modo é LOST mesmo dentro da coluna dos que já falhavam.

**R3 — o método de medição é o do repositório, não min-de-3.** Min de **cinco** dentro de um processo, alternado entre binários, declarado como par, em `bench/isolated/` — que já existe com dez binários, incluindo `activation_stacks.rs`, `object_new.rs`, `element_access.rs`, `loop_shapes.rs` e `string_boundary.rs`. `bench/analytic.ts` **ranqueia**, nunca pontua uma mudança. Limiares expressos como múltiplo do espalhamento medido (p10/mediana/p90), nunca como número redondo. Nenhum limiar de 5% neste plano: o piso de ruído desta máquina é 10%.

**R4 — toda linha do ground truth é medida também na forma que o inliner sintático recusa.** `obj.g(s)`, não `g(s)`. Isto é do Plano 3 e está certo: `call free function` a 3,21 ns não contém chamada nenhuma, e qualquer custo por chamada derivado por diferença contra essa linha é inválido — `plan.md §7.2` já diz isso.

**R5 — teto de arquivo é um custo de cronograma, não uma formalidade.** Todo sítio de edição nomeado pelos quatro planos já está acima do teto: `entry/functions.rs` 1379 (teto 500), `entry/mod.rs` 1262, `heap/region/mod.rs` 989, `entry/cache.rs` 824, `text/mod.rs` 642; `emit/expr.rs` 2247 (teto 1000), `emit/escape.rs` 1376, `emit/capture.rs` 1332; `lower/body.rs` 1514, `ir/inst.rs` 954. Regra: **toda etapa que toca um arquivo estourado começa por dividi-lo em pasta**, num commit `refactor:` cujo portão é IR byte-a-byte idêntico. Isso é dias na frente de cada etapa e está orçado abaixo.

**R6 — nenhuma etapa mede contra `bench/analytic.ts` sem antes consertar os seis defeitos de instrumento de `plan.md §7`.** Em particular a regressão de 2,7x em `string split 16` (1755,86 → 4799,01, com o bench byte-idêntico entre os dois pontos) é uma dívida do honesty floor e é o primeiro item devido, não uma etapa de performance.

---

## 4. As etapas

### Etapa 0 — instrumento e duas sondas que podem matar etapas deste plano [dias]

**Objetivo.** Não consertar nada. (a) Implementar R1/R2/R3 em `scripts/bench/` e `scripts/cross_runtime_check.sh`. (b) Escrever, para cada uma das 15 linhas do ground truth, **quantas operações há por iteração** — sem isso um alvo em ms não é conversível em ns e nenhum falsificador pode falhar pelo motivo certo. (c) Consertar os seis defeitos de `plan.md §7` e bissectar a regressão de `string split`. (d) **A sonda que decide a Fase 3:** ablação de proveniência. Medir o mesmo `Math.floor(x)` com `x` local de laço (1 `FloatUnary`) contra `x` chegando por guarda (0 `FloatUnary`, chamada JS completa, ~35 ns). Zero linhas de produção.

**Arquivos.** `scripts/bench/*`, `bench/analytic.ts`, `bench/isolated/src/bin/provenance.rs`.

**Falsificador.** A régua roda duas vezes sobre o **mesmo** binário e reporta espalhamento; se p90/p10 de qualquer linha exceder 1,25, essa linha não suporta limiar abaixo de 25% e isso fica escrito ao lado dela — a régua não é rejeitada, os limiares é que passam a ser dela. Para a sonda (d): se a diferença entre operando provado e operando por guarda for menor que 10 ns, a hipótese "proveniência responde por uma fatia grande do custo de chamada" está refutada e a Fase 5 desce na ordem.

**Ganho.** Zero, declaradamente. O que produz é a condição de falsificabilidade das seis fases seguintes e a atribuição correta entre "protocolo" e "proveniência" — que hoje está toda na coluna errada.

**Risco.** Nenhum para corretude. O risco é sondar virar desculpa; timebox de cinco dias e o resultado é escrito mesmo quando é "nada".

---

### Etapa 1 — A CHAVE: raízes precisas e a pilha vinda da máquina [meses, começa agora e roda em paralelo]

**Objetivo.** Ligar as três capacidades que já estão escritas, testadas e sem cliente. Três entregas, nesta ordem:

1. **Walker de pilha** em `rts-host`, ao lado de `stack.rs`, sobre `RtlCaptureStackBackTrace`/`RtlVirtualUnwind` — **não** sobre `rbp`. `preserve_frame_pointers` está ligado e a cadeia de frames compilados é caminhável, mas a cadeia **não é toda nossa**: entre um compilado e o próximo há `call_counted`, `called`, `invoke` e o nativo em execução, e nem Rust nem LLVM prometem manter `rbp` numa função que não precisa dele. O walker **salta** endereço que o `CodeMap` não atribui; um que parasse reportaria exatamente um frame.
2. **`.stack` vem do walker**, em *shadow mode*: `callees` continua presente e os dois traces são comparados por **igualdade** em todo fixture que compara stack, no corpus inteiro, antes de `callees` sair. Cai junto, de graça, o número de linha — `PositionMap` já é construído para toda função compilada e lido por ninguém.
3. **Raízes precisas por frame descriptor** substituem `scan_stack`, atrás de um modo de **stress GC** (coleta a cada alocação) rodado sobre os 808 arquivos antes de virar padrão.

**Arquivos.** `crates/rts-host/src/stack.rs` (+ pasta nova), `crates/rts-core/src/entry/roots.rs`, `crates/rts-core/src/entry/throw.rs`, `crates/rts-cranelift/src/observe/`, `crates/rts-cranelift/src/gc/`.

**Falsificador.** Escrito antes: um programa com três níveis de chamada JS separados por um nativo (`[1,2].map(f)` com throw dentro de `f`) produz `.stack` com **todos** os frames JS. Se o walker reportar menos frames que `callees` reporta hoje em **um** fixture, ele trunca, a etapa falhou, e `callees` não é removível — o que tira 7,3–10,2 ns do orçamento da Fase 3. Para as raízes: o programa de `element-load.md` (60 objetos, 20 000 alocações por passagem) sob stress GC tem de dar **0 errados**; e a suíte inteira sob stress não pode perder um arquivo.

**Ganho ancorado.** **Zero de benchmark, e isso é para ser dito.** O ganho é habilitador e o valor está medido do outro lado: destrava `ElementLoad`/`ElementStore` (Fase 4), destrava `Repr::I8/I16/F32` e a conversão de largura inteira que hoje não existe, destrava campos sem box, e legaliza remover as três pilhas de ativação que `native-call-floor.md §3a` prica por ablação em **7,3–10,2 ns por chamada** sobre um custo total de ~24,5. Entrega também o número de linha nos traces, que é feature de usuário.

**Risco.** É a maior superfície de dano do workspace: mexe no coletor. Duas mitigações estruturais, ambas obrigatórias: shadow mode para o trace (nada removido antes da igualdade valer no corpus), e stress GC como precondição de default. Sem essas duas, esta etapa é o `single_pass` de novo.

**Por que começa agora.** Porque toca `rts-host`, `rts-cranelift::observe` e `rts-core::entry::roots` — crates que a Etapa 2 não toca — e porque é o item de maior lead time do plano. Começar tarde faz dela o cronograma.

---

### Etapa 2 — Os impostos fixos que não dependem da chave [semanas, em paralelo com a 1]

Quatro itens, todos em `rts-core`, todos com causa lida no código, nenhum precisando de raiz precisa. Esta é a etapa que move números enquanto a Etapa 1 não move nenhum.

#### 2a — `Str::concat` deixa de copiar os dois lados

**Objetivo.** `text/mod.rs:487` faz `Vec::with_capacity(a+b)` e copia os dois lados **sempre**. Trocar pela forma do Hermes: buffer crescível compartilhado, `s = s + x` faz append e cunha só um header novo nomeando um **prefixo** — o resultado já nasce plano, então indexação continua O(1) sem passo de flatten. `CONCAT_STRING_MIN_SIZE = 256`; abaixo disso copiar continua certo. **Não** rope: a patologia de intercalar concat com indexação (Egorov) volta a ser quadrática e `string_idx` é uma das linhas.

**Arquivos.** `crates/rts-core/src/text/` (dividir antes: 642 linhas), `entry/context.rs` (`intern_value`), `entry/coerce/mod.rs:150`.

**Falsificador.** É teste de **forma**, não de tempo: reescalonar n = 5k/10k/20k/40k/80k/120k; a razão por duplicação tem de cair de 2,8/3,6/4,45/5,5 para ≤2,2 em **todos** os degraus. Um degrau acima refuta mesmo que o total caia. Segundo: `parts.push()+join("")` em n=100k, hoje em 15–24 ms, não pode piorar — se o buffer conserta `+` e deixa `join` quadrático, o programa real não ganhou nada. Terceiro: pico de working set do programa de 240 KB abaixo de 200 MB. Quarto, de corretude, e é o primeiro a escrever: **tomar um prefixo, fazer append no original, ler o prefixo** — tem de dar o texto de antes.

**Ganho ancorado.** A âncora é uma **razão**, não um valor: no mesmo binário, `parts.push()+join("")` custa 15–24 ms onde `+=` custa 4107–4182 ms — 174–274x. Aplicada à linha, isso põe `string_cat` na ordem de **225–355 ms**, não em 15–24. Dois dos quatro planos tomaram o valor absoluto do controle como alvo; isso é ~10x além da evidência e este plano não repete.

**O que 2a NÃO conserta, e está medido:** `string concat 2` custa **120,9 ns** para concatenar duas strings curtas contra 0,45 no node. Isso não é quadraticidade — é o piso de chamada nativa mais uma célula de 128 bytes por resultado. Sai na 2b e na Fase 3, não aqui.

**Risco.** Alto: imutabilidade é a invariante que um buffer compartilhado viola. Regra estrutural: **só o último prefixo do buffer pode apender**; qualquer outro caso copia como hoje. Mais teste de propriedade afirmando que a forma buffered e a plana do mesmo texto são indistinguíveis por todo método público de `Str`. `cross-runtime` é denso em string e não tolera paralelo — um processo por arquivo, e um LOST só vale depois de reproduzir o fixture à mão.

#### 2b — `well_known` memoiza seis nomes e cobra três SipHashes por todo o resto

**Objetivo.** `entry/mod.rs:213-221` lista exatamente seis nomes, e `context.rs:388` só escreve o memo se o nome estiver na lista. Um nome fora dela paga `Str::from_str` (um malloc) + `Interner::intern` = `units_hash` por code unit + duas sondas de `HashMap` + `same_units`. **Três SipHashes por acerto.** Cinco áreas independentes acharam o mesmo defeito.

**Arquivos.** `crates/rts-core/src/entry/context.rs`, `entry/mod.rs` (dividir antes: 1262 linhas), `entry/symbol.rs` (parar de `format!` os `@@` no sítio de chamada — `symbol.rs:83` já tem a forma certa).

**Falsificador.** `bench/isolated/src/bin/well_known.rs`: (a) scan linear de 6 `&str` — o acerto de hoje; (b) o mesmo com 12; (c) `Str::from_str` + hash + duas sondas — o erro de hoje. Decide as duas metades de uma vez: quanto vale mover um nome para a lista **e** quanto cada nome extra cobra de todo erro. Mais um contador `AtomicU64` de erros por nome em debug, que é profile-independente e transforma "cinco áreas acham isso quente" numa tabela de frequência sem build de release.

**Ganho ancorado.** As linhas: `flow generator next` 805,09 (dois nomes por `.next()`), `call closure make+call` 1672,46 (dois por closure), `binary alloc Uint8Array 64` 944,93, `binary TextEncoder 16` 1250,23, `string split 16` 4799,01, `regex replace` 1503,12, `prop instanceof` 240,80 — e, via `generator::result`, `array for-of/map/filter 16` e as iterações de Map/Set. Nenhuma dessas está nas 15 linhas do ground truth, e várias estão no topo da tabela real.

**Risco.** Alongar a lista alonga todo **erro**, e `regex/mod.rs:445` chama `well_known` com o nome de grupo de captura **do usuário** — erro garantido por grupo nomeado por match. A curva de custo do scan tem de ser medida, não assumida.

#### 2c — Três resoluções por execução que deviam ser por sítio

**Objetivo.** (i) `allocate_for_target` faz `well_known("prototype")` + `read_property` + `typed_as` a **cada** `new` — 41 dos 76,38 ns de `new P(i,1)`, mais da metade, sem tocar no alocador. Vira cache por sítio chaveado na célula do callee, invalidado por **versão** que incrementa só em escrita da propriedade `prototype`. (ii) `global_get` não tem cache nenhum: 14,7 ns para ler `Math`, `console`, `JSON`; num mundo fechado esses são fatos de compilação. Cache com versão do objeto global, porque `globalThis.x = f` é legal. (iii) `RuntimeOp::SetCallName` — uma travessia inteira **antes de toda chamada** cujo callee tem grafia, carregando um throw check que nunca pode precisar, cujo único leitor é o caminho de falha que redige um `TypeError`. `plan.md §S1` já tem a especificação completa: alargar `call_counted` (já em 7 params) com o índice do literal como operando e deletar a entry.

**Arquivos.** `entry/functions.rs` (dividir antes: 1379 linhas), `entry/global.rs`, `emit/call.rs:437-448`, `runtime/mod.rs`, `entry/table.rs`.

**Falsificador.** Cada sub-item tem um "antes" isolado e um limiar: `new P(i,1)` de 76,38 tem de cair para ≤40 ns só com (i); leitura de global de 14,7 para ≤3 ns; `SetCallName` está ablado em **2,3–2,9 ns por chamada nomeada** e tem de aparecer. Qualquer um que não caia 1,5x sozinho é **abandonado e escrito**, nunca "melhorado depois". Corretude, escrito antes de (i): um fixture que reatribui `C.prototype` entre dois `new` e exige o protótipo novo no segundo.

**Ganho ancorado.** (i) ~41 ns por `new`, medido por diferença contra o literal. (iii) 2,3–2,9 ns em toda linha de chamada nomeada da tabela — `call method` 29,35, `prop proto method call` 27,79, e todos os métodos de built-in. **Não** em `call free function`/`call arrow`: essas linhas não contêm chamada.

**Risco.** (i) é o único com risco real: `C.prototype = X` entre dois `new`, `setPrototypeOf`, `.bind()` encadeado, `Reflect.construct` com `newTarget`. A invalidação é por versão da propriedade, não por qualquer escrita. Para (iii), `CoreEntry` tem numeração densa e asserta `ALL.len() == CORE_ENTRY_COUNT`; `table.rs:32-37` proíbe renumerar na remoção e a remoção precisa do tratamento que a tabela já prescreve.

#### 2d — Construir um array custa ~215 ns e oito linhas pagam

**Objetivo.** `alloc array literal 4` = 231,34; `array index read` = 16,63 explica a leitura; ~215 ns é `array::built_in`. Dentro: o malloc do `Vec` + `Slab::insert`; `Region::alloc` escrevendo **quinze palavras zero** por célula no caminho de free list; `set_length` chamando `refuses_key_write` **três vezes**, cada uma um `integrity_at` + um find linear, mais `reconcile_length` re-derivando a chave de comprimento; e o sweep — `collect_cycle::release` faz **22** `Aside::remove` por célula liberada, e em regime estacionário há um `release` por `alloc` de free list.

**Arquivos.** `entry/array.rs`, `entry/objects.rs`, `heap/region/mod.rs`, `entry/collect_cycle.rs`.

**Falsificador.** Decompor **antes** de escolher sub-item: `crates/rts-core/examples/array_build.rs`, quatro laços de 10⁶ — `built_in` completo, `built_in` sem `set_length`, `release` sobre célula morta, `region.alloc` sozinho. Quatro números decidem se o alvo é alocação, `set_length` ou sweep. Nada disso precisa de TypeScript nem de build de workspace.

**Ganho ancorado.** Oito linhas: `alloc array literal 4` 231,34, `call varargs 3` 253,36 (= `call method` 29,35 + ~215 + <10 específico de rest — ou seja **o penhasco de aridade é construção de array, não convenção**), `coll Object.keys 4` 308,09, `binary subarray 64` 294,13, `map`/`filter`, `string split 16` 4799,01, `regex exec+group` 2268,25, `json stringify small` 5014,88.

**Risco.** Duas armadilhas já documentadas: **não** limitar `trace::edges_of` pelo shape (`trace.rs:141-158` anda `width−1` porque o último slot guarda o **endereço** do bloco de overflow, e `shape_of` não é predicado confiável de "tem shape"); e **não** tirar a não-enumerabilidade de `length` de dentro de `set_length` — é o funil único que a registra, e desviar dele torna `Object.keys(arr)` e `for-in` errados.

---

### Etapa 3 — A chamada vira uma chamada [semanas, DEPOIS da Etapa 1]

**Objetivo.** Nesta ordem, e a ordem importa. (a) **Decompor primeiro**: `call method` a 29,35 é ~28 ns acima do piso para um corpo `return x + this.v`. Componentes nomeados somam 10–12 ns; **dezesseis nanossegundos não têm dono** e toda linha de método os paga. `crates/rts-core/examples/entry_cost.rs` estendido pra `entry::call` mais `bench/isolated/src/bin/vec_markers.rs` fecham isso em uma tarde. (b) Remover as três pilhas de ativação (`pending_arguments`, `pending_counts`, `callees`) — **legal agora**, e só agora, porque o walker da Etapa 1 sabe o que está rodando e as raízes precisas cobrem o que elas cobriam. (c) `Inst::Call` onde o mundo fechado prova o callee: exatamente o par que `inline::candidates` já calcula (`declarations_of(body,name)==1` + `primordial::untouched`), mais "binding nunca atribuído". Funções JS já são `Linkage::Local`, colocated, `call rel32`. (d) `Inst::CallIndirect` — que existe, é baixado e tem **zero** produtores — atrás de guarda de que a célula é callable, com fallback para `call_counted` para proxy, bound, nativo e não-callable.

**Arquivos.** `crates/rts-codegen/src/emit/call.rs` (hoje `:501` termina **toda** chamada em `RuntimeOp::Call`), `emit/function.rs`, `crates/rts-core/src/entry/functions.rs`, `crates/rts-cranelift/src/ir/builder.rs`.

**Falsificador.** Escrito antes, e separa duas explicações rivais: um sítio monomórfico com `CallIndirect` guardado tem de bater `__rts_call_counted` por pelo menos 2x. Se não bater, os ~28 ns **não são a porta** e são os dezesseis não atribuídos — e a etapa muda de alvo antes de escrever mais código, exatamente como o repo já fez ao refutar os `with_current` por ablação. Segundo: `rts ir` sobre a folha não pode mostrar `Call __rts_thrown_address`. Terceiro, de corretude: `1()` continua lançando `TypeError` **capturável**, porque o caminho de falha do guard é a porta de hoje.

**Ganho ancorado.** As três pilhas estão abladas em **7,3–10,2 ns**. `SetCallName` sai na 2c. Isso é ~10–13 dos ~24,5 ns de uma chamada de método real. O resto depende de (a). **Não** alego paridade: o node faz `c.m(a)` em 0,35 ns e nada neste plano chega perto disso.

**Risco.** `type Compiled = extern "C" fn(u64 x6) -> u64` é um transmute entre crates; assinatura errada não é resposta errada, é salto com pilha corrompida. `rts-host` já afirma a concordância das constantes e essa afirmação é o que transforma discordância em recusa. `rts-napi`, `eval` e bound functions cunham callables que o emissor nunca viu: o kind desses é o conservador, nunca "plain". E o corpus `obfuscated` entra no portão por nome — um ofuscador emite exatamente a forma de chamada que nenhuma prova sintática esperava.

**Não alargar `ARGUMENT_SLOTS`.** O penhasco de 4→5 argumentos é `array::built_in` (2d), não convenção: `call varargs 3` = `call method` + ~215 + <10. Alargar move o penhasco e, no x64 do Windows, com apenas quatro registradores de argumento inteiro, põe parâmetros na pilha em **toda** chamada. A saída certa está escrita no próprio `functions.rs` — um vetor de argumentos em slot de pilha do chamador — e é capacidade de máquina, não uma constante maior.

---

### Etapa 4 — Elementos [semanas, DEPOIS da Etapa 1]

**Objetivo.** O armazenamento do array para de ser dois níveis de tabela lateral (`array_elements: Aside<Slot>` + `arrays: Slab<Vec<u64>>`) e passa a ser endereçável **a partir da célula** — que é a quarta das cinco correções que `element-load.md` lista e a única que ele chama de "the real answer". Com isso: `Inst::ElementLoad` ganha produtor real em `emit/property.rs` (hoje `:174-178` **desvia do cache de propósito** para chave provada numérica e emite `Call __rts_get_indexed`), e `Inst::ElementStore` passa a existir. Crescimento `old + (old>>1) + 16`. **Um** elements kind, não os 21 do V8: corrida compacta de palavras, com modo dicionário desde o primeiro dia. Reservar o byte `kind` no cabeçalho mesmo sem usar, para não migrar o cabeçalho duas vezes.

**Arquivos.** `crates/rts-core/src/entry/array.rs`, `entry/mod.rs`, `crates/rts-codegen/src/emit/property.rs`, `crates/rts-cranelift/src/ir/inst.rs` + `lower/body.rs` (ambos exigem divisão antes).

**Falsificador.** O primeiro é de corretude e é o programa que já existe: 60 objetos, 20 000 alocações por passagem, sob **stress GC** — 0 errados, contra os 53 de 60 que a versão anterior produziu. Rodar também com closures (57 de 60 antes). Só então tempo: `a[i]` abaixo de 5 ns. Se ficar entre 6 e 15, o custo era a tabela lateral e não a travessia, e a ordem interna estava errada. Seis formas nomeadas antes do código: buraco, esparso, `length` atribuído, `delete a[i]`, Proxy sobre array, typed array. E `a[50000000]=1` não pode piorar os 400 MB de hoje.

**Ganho ancorado.** O teto está medido de um jeito raro: o caminho de `for`-`of` mede 6–8 ns por elemento **incluindo a cópia integral** do array que `iterate` faz, contra 15–16 ns para `a[i]`. Copiar o array inteiro e percorrê-lo pelo caminho rápido custa metade de lê-lo com `a[i]`, o que põe a carga sozinha bem abaixo de 6 ns.

**Risco.** Maior risco de resposta errada silenciosa do plano, e já foi pago uma vez. Mitigação estrutural além do stress GC: `elements_mut` devolve um tipo-guarda cujo `Drop` reescreve ponteiro/len/cap na célula, de modo que um caminho de mutação que **esqueça** de reescrever não seja expressável. (Ideia do Plano 1, e é a melhor mitigação do conjunto.)

---

### Etapa 5 — A prova atravessa o bloco [semanas]

**Objetivo.** A versão barata da C2, e o número diz por quê. Em `bench/analytic.ts` há **794** guards; apenas **137 (17%)** têm parâmetro de bloco como entrada. Os 657 locais não são deste trabalho — `ir/fold.rs` já os come. O ganho **direto** de fazer a prova sobreviver a uma fronteira de bloco é 0,65 ns. O ganho **indireto** é a conta inteira: `machine_operation` — o único lugar onde este motor troca uma chamada de biblioteca por uma instrução — exige operando **já provado** e portanto está desligado para essencialmente todo código real, em silêncio, porque cair no caminho comum é uma resposta correta. `Math.floor(x)` é uma instrução quando `x` é local de laço e ~35 ns quando `x` veio de qualquer outro lugar. `bench/monte_carlo_pi.ts` gasta 705 de 790 ms no laço, é **inteiramente anotado `: number`**, e as anotações não provam nada ali.

Concretamente: o join de `property::emit_read` (`property.rs:87`) e o de `emit_guarded` (`expr.rs:1869-1877`) param de usar parâmetro `UNPROVEN`; `binding::read` (`binding.rs:89`) para de converter ligação I32 de volta para F64 na leitura.

**Arquivos.** `crates/rts-codegen/src/emit/property.rs`, `emit/expr.rs` (dividir antes: 2247), `emit/binding.rs`, `crates/rts-cranelift/src/ir/func.rs` (ganha substituição/remoção — 395 linhas, cabe).

**Pré-requisito de corretude, obrigatório e escrito.** `plan.md §8.6`: `Math.sqrt(g())` **avalia `g` duas vezes** quando a repr do argumento recusa a operação de máquina — `call.rs:211` emite o argumento antes da recusa em `:212-214`, e `:88-90` re-emite a chamada inteira. Imprime `2 2` aqui e `2 1` no node e no bun. **Isso tem de ser consertado antes de qualquer mudança em `machine_operation`.**

**Falsificador.** Escrito antes: o par mínimo do `Math.floor` — a versão com o operando vindo de um `let` de módulo capturado tem de emitir **1** `FloatUnary`, como a versão com local de laço. Se emitir e o tempo não mover, a atribuição da Etapa 0(d) estava errada. Segundo, e é a refutação já registrada: forçar o contador a I32 com `i=(i+1)|0` **piorou** o laço de 119 para 144 ms, porque cada leitura acrescenta um `ToF64`. **Nenhuma** das 15 linhas pode regredir, nem uma; uma banda inteira pela metade é pior que nenhuma.

**Ganho ancorado.** Direto: 0,65 ns por guard cross-block, 137 deles. Indireto: `Math.floor`/`sqrt`/`abs` de ~35 ns para uma instrução onde o operando hoje chega por guarda — e `monte_carlo_pi` emite **19 `Widen` e 25 `Guard` para 12 operações float reais**. `ToInt32` custa 3,1–3,7 ns por ocorrência; o box em si custa ~1 ns de ida e volta, e quem lê o IR chuta ao contrário.

**Risco.** Uma prova que sobrevive a um merge onde não deveria é uma resposta errada. A regra do RULE 4 do README do `rts-codegen` é o contrato: anotação é evidência, prova onde a linguagem pode checar, vira **guarda** onde não pode — o caminho do Static TypeScript, não o sistema sound do Static Hermes.

---

### Etapa 6 — CONDICIONAL: efeito na instrução e passes [meses]

**Objetivo.** Só se a Etapa 5 mostrar que o gap restante é de passe. `Inst` ganha campo de **efeito** (`Effects: None` numa operação tipada contra "may read and write memory" numa genérica), `ir::Function` já ganhou reescrita na Etapa 5, e só então GVN/LICM/DCE/const-fold sobre o IR próprio, com `MemFlags`/`AliasRegion` corretos descendo para o Cranelift.

**Falsificador.** Escrito antes: se a soma das Etapas 1–5 não trouxer a média geométrica do ground truth para dentro de 5x do melhor entre node e bun, a Etapa 6 **não fecha a diferença** — E1 mede toda a especialização por tipo sobre ICs que funcionam em 3,86x — e a resposta não é escrever mais passes, é voltar e perguntar qual protocolo continua caro.

**Risco.** Um efeito declarado como `None` numa operação que toca memória é resposta errada silenciosa. Efeitos são declarados na **definição** da instrução, nunca por um sítio; o verificador de `run.rs:1087` passa a rodar depois de cada passe; e cada passe tem desligamento individual para bissecar um LOST. E o maior programa do workspace entra no portão por nome — o `single_pass` passou 800 arquivos e segfaultava o editor do `rts-game` 1,5 a 3 segundos dentro, todo run.

---

## 5. O que NÃO fazer

**Ligar o mid-end do Cranelift.** Não é pergunta aberta: já foi respondida. `speed` moveu placement de 55 para 70 ms e **não moveu `run`**; o README diz que move Monte Carlo por menos que o ruído entre corridas, e a causa está escrita — *o mid-end não enxerga através de uma chamada opaca e o IR deste motor é quase só chamada opaca*. Três dos quatro planos gastam uma etapa medindo isso no minuto exato em que é garantido dar zero, e um deles escreve a conclusão permanente errada. O comentário em `target/mod.rs` já diz quando volta a valer: *o dia em que o runtime deixar de ser uma chamada por operação*. Ou seja: depois da Etapa 3, não antes. **Há uma pergunta real ali que ninguém pegou:** o caminho AOT pede `Priority::CodeQuality` e recebe o otimizador desligado, e o próprio comentário diz "no stated argument covers that". Isso é um item, e é de uma tarde.

**Micro-otimizar as cargas do inline cache.** Poirier/Rohou/Serrano implementaram dynamic binary modification no Hopc exatamente para chegar ao código que um JIT emite: −1,5% a −10% de leituras e **zero** aceleração, porque o out-of-order já executava os loads cedo. Bate com a nota de memória deste repo sobre piso de ruído de layout.

**Otimizar o sweep como item independente, ou escrever write barriers.** Curva plana entre 0 e 25 ciclos medida aqui; card marking em 0,9% ± 0,8% (Blackburn). O sweep entra na 2d **como parte da decomposição de `array::built_in`**, onde `collect_cycle::release` faz 22 `Aside::remove` por célula liberada — não como campanha de GC.

**Coletor móvel e ponteiros diretos.** A Etapa 1 entrega raízes precisas, o que abre a porta; atravessá-la é outro trabalho e não aparece em nenhuma linha medida. O RTS paga a indireção por índice sem receber mobilidade: dívida real, nomeada, não paga aqui.

**O sistema de tipos sound do Static Hermes.** Preço publicado: recusa spread em chamada, rest em destructuring, métodos async e generator, chave computada em classe, chamada opcional e `==`. Isso perde arquivos dos 746 **por construção**.

**`Cell<*mut Context>` no lugar do `RefCell<Vec<Context>>`.** Já experimentado e documentado: vale 0,53 ns, e rebaixaria uma invariante checada (sem empréstimo reentrante, que oito módulos de `rts-core` assumem) a asserção de debug — UB silencioso em release por meio nanossegundo.

**Colapsar `with_current`.** Dois negativos medidos independentes: um colapso custou 7,6 pontos em `instanceof`; outro moveu `{a:1}` de 1942 para 2084 ns.

**Trocar a convenção de chamada ou usar `Convention::Tail`.** 0,3–0,6 ns por contagem de instrução, e decisivamente: `call free function` e `call method` emitem sequências de sete operandos **byte-idênticas**, então os 26 ns entre eles provadamente não são a convenção.

**Dimensionar célula de IC por espécie.** Tentado e revertido: resolvedor de escrita gravando seis palavras ao lado de sítio de leitura dimensionado para três — corrupção de memória, não resposta errada.

**Afrouxar a recusa de `cache.rs` para receptor string.** A recusa **não é** "é string": é "não há link registrado", e ela existe porque `inherited_from` **substitui** o protótipo por espécie para arrays, callables, texto e objetos simples — um array e um objeto literal que guarda `length` chegam hoje à mesma forma e ao mesmo tipo, e um sítio cacheado contra um reconheceria todos os outros. O conserto certo é dar às células de texto um link **próprio**, e ele exige primeiro recusar `Reflect.setPrototypeOf("abc", …)`, que hoje **funciona** (`"abc".foo` responde 42 depois). E o teto está medido: o análogo em árvore (armar `length` de texto) valeu **12,8 ns**, não 68; e `cache_resolve_indirect` recusa `slot >= holder_width`, então só as 14 primeiras propriedades de `String.prototype` — de 49 — são alcançáveis, e `split` está no índice 21. Faça a experiência de custo zero primeiro: cronometrar `s16.toUpperCase()` contra `new String("abcdefghijklmnop").toUpperCase()`. O wrapper tem shape e link registrados; o corpo nativo é byte-idêntico. **A diferença entre essas duas linhas é o candidato inteiro**, medida pelo instrumento de registro, sem mudar uma linha do motor.

**Estreitar `capture.rs` agora.** É real — 42x no caso try-em-laço, 3,6–5,2x no sombreamento — mas a direção do erro **se inverte** ao estreitar: hoje sobre-incluir custa uma carga; sub-incluir é duas closures discordando sobre uma variável, sem crash para anunciar. Não é uma etapa de "dias" com o corpus como portão. Entra depois da Etapa 3, com os fixtures escritos **primeiro** (shadowing com `var` em bloco, catch que lê nome escrito no try, finally que lê nome escrito no catch — este último o próprio comentário já registra como bug histórico).

**Consertar o O(n²) de `proven::analyse` como etapa de performance.** É 26x real (121,7 contra 4,6 ms), mas de tempo de compilação, e compilar é 0,41% do wall clock. É bug de ergonomia; entra como `fix:`, não com orçamento de etapa.

---

## 6. Estado final esperado, com a incerteza dita

Três coisas antes dos números.

**Primeira: as 15 linhas não são o programa.** Elas omitem `regex exec+group` 197x, `string split 16` 137x, `regex test` 78x, `alloc Uint8Array` 62x, `map` 47x, `filter` 42x, `generator next` 30x, `template literal` 27x, `TextEncoder` 26x, `JSON stringify` 20x, `JSON parse` 12x. A Etapa 2b toca a maioria dessas (nomes bem-conhecidos por chamada) e a 2d toca as de construção — mas várias são **implementação de biblioteca em `rts-core`** e não saem de nenhuma etapa deste plano. Isso é um bloco de trabalho que ainda não tem plano, e dizer isso vale mais que estimá-lo.

**Segunda: nenhuma linha em ms é conversível hoje.** A Etapa 0(b) produz os divisores por iteração; até lá, todo alvo abaixo é uma faixa e não um ponto.

**Terceira: `flow throw+catch` está em 0,14x** — sete vezes mais rápido que o node. Nenhum portão deste plano protege uma vantagem existente até a R2 entrar. Depois dela, uma linha que **melhora demais** também é investigada, porque normalmente significa que alguma coisa deixou de acontecer.

### Confiança ALTA (mecanismo lido no código, âncora interna medida, alternativa refutada)

- `arith_f64` (1,2x) e `math_call` (1,5x) ficam onde estão; já estão certos.
- `string_cat` **sai do quadrático** — o falsificador é a razão por duplicação cair de 5,5 para ≤2,2, teste de forma que não depende de estimativa. Valor: ordem de **225–355 ms**, dentro da razão 174–274x que o controle `join()` já mede no mesmo binário. Contra os 13 ms do node isso ainda é **~20–30x**, e nenhuma etapa promete competitividade ali.
- `new P(i,1)` de 76,38 para ≤40 ns só com a 2c(i), sem tocar no alocador nem no coletor.
- Toda linha de chamada nomeada cai 2,3–2,9 ns com a 2c(iii), ablado.
- **Número de linha nos traces de erro** — feature de usuário, cai de graça da Etapa 1.

### Confiança MÉDIA (âncora existe mas é de outro contexto ou é ablação parcial)

- `call_method`/`call_closure`: as três pilhas de ativação valem 7,3–10,2 ns ablados sobre ~24,5 ns de chamada real. Com a 2c(iii) junto, isso é ~10–13 ns dos 24,5. O resto depende dos **dezesseis nanossegundos não atribuídos** que a Etapa 3(a) tem de decompor **antes** de escrever código. Faixa honesta: 24,5 → 10–15 ns por chamada. O node faz em 0,35.
- `array_idx`: `a[i]` de 15–16 para ≤5 ns, ancorado nos 6–8 ns que o `for`-`of` já entrega **pagando uma cópia integral do array**.
- `alloc_obj` e as oito linhas de construção de array: a 2d decompõe antes de prometer. `alloc array literal 4` de 231 para a ordem de 60–100, se o sweep for a fatia que a decomposição indicar.

### Confiança BAIXA ou NÃO ANCORADA — e a honestidade está aqui

- **`prop_read` era dado como contradição não resolvida; foi resolvido — ver §0.2.** O RTS lê uma propriedade em **2,0 ns**, o que confirma os 3,3 ns e é saudável. Os 12,8x comparam 10⁸ leituras contra as ~zero que o V8 faz depois de içar as invariantes do laço. A linha **muda de dono**: sai do modelo de objeto e do cache, entra no LICM, ou seja na Etapa 6. Nenhum trabalho de cache de propriedade a move, e a Etapa 0 não precisa mais decidir isso.
- **`arith_i32` fica onde está até a Etapa 5**, e talvez depois. O Static Hermes chega a 4 ms no `calc.js` com o contador em **double** e não tem instrução aritmética de inteiro nenhuma — então F64 não é a causa; o round-trip `ToInt32` (3,1–3,7 ns por ocorrência) é. E forçar a banda pela metade já piorou o laço aqui, medido.
- **`string_idx`**: 1,68x é tudo que está ancorado, e o análogo em árvore valeu 12,8 ns, não 68. Dizer 56x aqui seria inventar.
- **A Etapa 1 não move nenhum benchmark**, e é a mais cara do plano. Ela se paga inteiramente no que destrava. Se as Etapas 3 e 4 não medirem, ela terá comprado corretude e números de linha por meses de trabalho — o que é defensável, mas não é o que você pediu, e o falsificador dela é justamente isso.

### O chão realista

**2x–8x do melhor entre node e bun na maioria das linhas estruturais, com `string_cat` em ~20–30x e duas linhas (`arith_i32`, `prop_read`) sem previsão.** Isso não é "desfrutar 100% da máquina", e este plano diz isso em vez de prometer. O teto publicado para um AOT sem JIT é conhecido — Hopc mede 1x–2x do V8 no JetStream rodando ~45 passes, dos quais 10 só de análise de tipo — e este plano roda zero passes até a Etapa 6, por escolha e com o número da Etapa 5 como condição.

O que ele entrega e nenhum dos quatro planos entregava: **as oito capacidades mortas ganham cliente**. Depois da Etapa 1, `ElementLoad`, `ElementStore`, `Repr::I8/I16/F32`, conversão de largura inteira, campos sem box, chamada direta e `describe_frames` deixam todos de ser ilegais ao mesmo tempo. Essa é a única mudança deste plano que muda o que o motor **pode** fazer, e é a que os quatro planos agendaram como consequência de outra coisa.

---

*Arquivos-chave para a próxima sessão, todos absolutos:*
`C:\Users\danie\Documents\GitHub\rts\docs\engine\the-unwired-keystone.md` ·
`C:\Users\danie\Documents\GitHub\rts\docs\codegen\element-load.md` ·
`C:\Users\danie\Documents\GitHub\rts\docs\codegen\the-missing-pass.md` ·
`C:\Users\danie\Documents\GitHub\rts\docs\codegen\native-call-floor.md` ·
`C:\Users\danie\Documents\GitHub\rts\docs\codegen\action-table-2026-08-26.md` ·
`C:\Users\danie\Documents\GitHub\rts\docs\codegen\plan.md` (§7 defeitos do instrumento, §8 bugs bloqueantes, §9 onze refutações) ·
`C:\Users\danie\Documents\GitHub\rts\crates\rts-core\src\entry\roots.rs:124` ·
`C:\Users\danie\Documents\GitHub\rts\crates\rts-core\src\entry\cache.rs:658` ·
`C:\Users\danie\Documents\GitHub\rts\crates\rts-core\src\text\mod.rs:487` ·
`C:\Users\danie\Documents\GitHub\rts\crates\rts-cranelift\src\target\mod.rs:1080`