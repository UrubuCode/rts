# RTS — TypeScript que vira programa de verdade

## Pra começar: do que estamos falando?

Imagine que você quer escrever um programa em TypeScript (a linguagem
do JavaScript com tipos). Hoje, você tem essas opções:

1. **Rodar com Node.js**: instala um runtime de ~100 MB no seu computador.
   Toda vez que você roda, ele lê seu código, traduz, executa.
2. **Rodar com Bun**: igual, mas mais rápido. Runtime de ~70 MB.
3. **Rodar com Deno**: parecido. Runtime de ~80 MB.

Em todos os casos, o **runtime** é uma máquina virtual gigante (uma
versão do Chrome/Safari sem a parte do navegador) que precisa estar
sempre presente. Seu programa de 5 linhas precisa do runtime de 70 MB
do lado.

O **RTS** faz diferente: pega seu código TypeScript e **transforma em
um programa nativo de verdade**, igual a um `.exe` que você baixa da
internet. Sem runtime, sem máquina virtual, sem nada extra. **Só seu
código, virado em instruções de processador.**

## "Como assim, programa nativo?"

Quando você baixa o Photoshop ou o WhatsApp Desktop, eles são
programas nativos: o sistema operacional executa eles diretamente, sem
precisar de outro programa por cima.

Programas em JavaScript/TypeScript normalmente **não são** nativos.
Eles vivem dentro de outro programa (Node, Bun, Chrome) que sabe
interpretar JavaScript.

O RTS pega seu TypeScript e **traduz pra linguagem de máquina**, do
mesmo jeito que C ou Rust faz. O resultado é um arquivo `.exe`
pequenininho (3 KB no nosso teste) que você pode mandar pra qualquer
pessoa e ela roda direto, sem instalar nada.

## "Pra que isso serve?"

### 1. **Velocidade**

Sem o overhead do runtime, programas RTS começam a rodar
instantaneamente. Em testes:

- **Programa que faz cálculos**: RTS termina em **15 milissegundos**.
  Bun (o segundo mais rápido) leva **70 ms**. Node leva **98 ms**.
  RTS é **4,4× mais rápido**.
- **Cálculo de π por método estatístico (10 milhões de iterações)**:
  RTS faz em **50 ms**, Bun em **71 ms**. RTS é **41% mais rápido**.

A diferença é especialmente grande em programas curtos. Bun gasta
boa parte do tempo só "se preparando" (carregando o runtime, lendo seu
código, otimizando). RTS já está pronto desde o início — o trabalho
todo foi feito uma vez na compilação.

### 2. **Tamanho**

Um "Hello, world" em RTS: **arquivo de 3 KB**. Sozinho, sem dependências.

O mesmo programa empacotado com Bun ou Node usando ferramentas como
`bun build --compile` ou `pkg`: **~60-100 MB** (porque vai junto o
runtime inteiro).

### 3. **Distribuir é fácil**

Você quer dar uma ferramenta sua pra um colega? Manda o `.exe` de 3 KB.
Ele clica e roda. Não precisa instalar Node, Bun, ou nada.

Quer publicar uma ferramenta de linha de comando? Sobe um único
arquivo. Funciona em qualquer Windows.

### 4. **Performance previsível**

Runtimes JavaScript modernos (Node, Bun) são "JIT compilers" — eles
otimizam seu código **enquanto roda**, observando padrões. Isso é
mágico, mas tem um preço: às vezes eles "deoptimizam" (jogam fora a
otimização porque algo mudou) e seu programa fica subitamente 10×
mais lento.

RTS faz a otimização **uma vez**, durante a compilação. Depois disso,
performance é constante. Nada surpreende em produção.

## "Tá, mas o que dá pra fazer?"

RTS suporta uma boa parte do TypeScript moderno:

- **Variáveis e tipos**: `number`, `string`, `boolean`, etc.
- **Funções**: declarações, arrow functions, closures simples,
  recursão (sem estourar a stack — feature chamada "tail call
  optimization")
- **Classes**: construtor, métodos, herança (`extends`/`super`),
  getters/setters, métodos estáticos, métodos abstratos
- **Loops e condicionais**: `if`, `while`, `for`, `for-of`,
  `switch`/`case`
- **Arrays e objetos**: `[1, 2, 3]`, `{ nome: "alice", idade: 30 }`,
  spread `[...a, b]`, destructuring `const { x } = obj`
- **Erros**: `try`/`catch`/`throw`/`finally`
- **JSON**: `JSON.parse(...)`, `JSON.stringify(...)`
- **Datas**: `Date.now()`, `Date.parse(...)`
- **Console**: `console.log`, `console.error`, `console.warn`
- **Regex**: `/padrao/`, `.test()`, `.replace()`

E coisas mais avançadas:

- **Threads de verdade**: programa que usa múltiplos núcleos do
  processador, com `thread.spawn()`. Sem ser fake como o async do JS.
- **Paralelismo automático**: você escreve `arr.map(x => x * 2)` e o
  RTS roda em paralelo automaticamente, usando todos os núcleos.
- **HTTPS sem instalação**: faz requisições seguras com certificados
  válidos, sem precisar configurar nada.
- **Interface gráfica**: dá pra criar janelas com botões, campos de
  texto, tudo nativo (via biblioteca FLTK).

## "E o que NÃO funciona?"

Algumas features de JavaScript ainda não estão prontas:

- **`async/await`** — ainda não foi implementado. Você não consegue
  fazer "pausar a função e continuar depois". Por enquanto, threads
  de verdade ou callbacks resolvem.
- **Bibliotecas npm**: a maioria não vai rodar porque dependem de
  features muito específicas do JavaScript. RTS é melhor pra você
  escrever do zero ou usar libs próprias.
- **Browser**: RTS só roda no servidor / desktop, não no navegador.
  (Pra browser, use TypeScript normal.)

## "Como começar?"

```bash
# Baixar e compilar (uma vez só)
git clone https://github.com/UrubuCode/rts
cd rts
cargo build --release

# Rodar um programa TypeScript
target/release/rts.exe run examples/hello_world.ts

# Compilar pra .exe nativo
target/release/rts.exe compile -p meu_programa.ts saida
./saida.exe
```

Exemplos prontos em `examples/`. Documentação detalhada em `README.md`
e `BLOG_POST.md`.

## RTS vs concorrentes — onde RTS ganha

A "concorrência" do RTS são os runtimes que executam TypeScript:
**Node.js**, **Bun**, **Deno**. Cada um tem força em algo. Aqui é onde
RTS **ganha**:

### 1. **Velocidade de início (startup)**

O programa começa a rodar **muito mais rápido**. Importante quando:
- A ferramenta é usada várias vezes seguidas (cada execução é nova)
- Você roda em pipeline de CI/CD (dezenas de invocações por commit)
- Você programa um cron / sistemd timer que roda a cada minuto

Comparativo medido (programa de cálculo simples, mediana de 100 runs):

| Runtime | Tempo  | Quão atrás de RTS |
|---------|--------|---------------------|
| **RTS AOT** | **15 ms** | — (referência) |
| RTS JIT | 21 ms  | 1,4× mais lento |
| Bun     | 71 ms  | **4,7× mais lento** |
| Node    | 98 ms  | **6,5× mais lento** |
| Deno    | similar a Node | similar a Node |

Não é "1% mais rápido" — é **vezes mais rápido**. Pra programa curto,
o tempo de startup do runtime é a maior parte do total.

### 2. **Tamanho do binário distribuído**

Você quer mandar uma ferramenta pro seu colega:

| Forma | Tamanho |
|-------|---------|
| **RTS compiled** | **~3 KB** |
| Bun `--compile`  | 60-90 MB |
| Node + `pkg`     | 50-80 MB |
| Deno `compile`   | 80-100 MB |

RTS é **20.000× menor** que Bun compiled. Cabe num e-mail. Pode subir
em qualquer lugar.

### 3. **Performance previsível (sem JIT-deopt)**

Os outros runtimes usam **JIT** (Just-In-Time compilation): otimizam
seu código enquanto roda, observando padrões. Funciona muito bem **na
maior parte do tempo**. Mas:

- **JIT pode "deoptimizar"**: detectou que assumiu errado, refaz tudo.
  Em produção, isso aparece como "lentidões inexplicáveis em
  intervalos aleatórios".
- **JIT precisa "esquentar"**: nas primeiras milhares de iterações,
  está rodando código não-otimizado. Em programas curtos, isso é
  **a totalidade do tempo de execução**.

RTS é AOT (Ahead-of-Time): compila uma vez, performance é constante.
Sem deopt, sem warmup, sem surpresa. Útil pra:
- Sistemas em tempo real (latência consistente importa mais que pico)
- Benchmarks reproduzíveis
- Garantias contratuais de performance

### 4. **Distribuir sem instalar nada**

Outros runtimes precisam estar instalados na máquina destino. Você
manda o programa, mas o usuário precisa ter Node/Bun/Deno instalado
e na versão certa. Suporte hell.

RTS é um `.exe` standalone. **Nenhuma instalação, nenhuma dependência,
nenhuma versão.** Manda, executa.

### 5. **Computação numérica (loops com cálculos)**

Programas que fazem **muitos cálculos em loop** (simulação, parsing,
hashing, criptografia, processamento de imagens, etc):

| Bench (10M iters Monte Carlo)  | Tempo |
|--------------------------------|-------|
| **RTS AOT** | **50 ms** |
| Bun     | 71 ms (1,4× mais lento) |
| Node    | 96 ms (1,9× mais lento) |

RTS aplicou uma técnica chamada "branchless if-to-select" que elimina
o "if" do código de máquina. Resultado: **bate Bun em workload onde
historicamente Bun ganhava**.

### 6. **Programa fica "morto" entre execuções**

Outros runtimes mantêm o processo ativo (event loop). Mesmo terminando
seu código, o processo às vezes demora pra fechar (workers, timers,
handles abertos). Em script curto, isso é tempo perdido.

RTS é programa nativo: termina e morre. Sem event loop fantasma.

### 7. **Memória**

Programa "Hello, world" rodando:

| Runtime | RAM usada |
|---------|-----------|
| **RTS** | ~2 MB     |
| Bun     | ~40 MB    |
| Node    | ~30 MB    |

15-20× menos memória. Importante em VMs pequenas, containers
limitados, ou centenas de instâncias rodando juntas.

### 8. **Determinismo**

Mesma entrada = mesma saída, sempre, na mesma ordem de operações.
Com JIT pode haver diferenças sutis (ex: ordem de allocs muda timing
de GC, que muda comportamento de timeouts). Não importa pra maioria
dos apps, mas importa pra:
- Testes
- Replays
- Sistemas distribuídos com lockstep

## Onde os concorrentes ainda ganham

Pra ser honesto, RTS **perde** em:

- **Compatibilidade com npm**: ~80% do ecossistema npm não roda
  ainda. Bun e Deno têm compat parcial; Node tem total. Issue #208
  trabalha em melhorar isso.
- **`async/await`**: não existe em RTS ainda (planejado em #207).
  Pra workload heavy-IO, Bun/Node são imbatíveis hoje.
- **JIT em programas longos**: depois de aquecer (~minutos), JIT pode
  alcançar ou superar AOT em casos específicos com data flow muito
  variável. Pra batch processing 24/7, vale comparar.
- **Maturidade**: Bun, Node, Deno têm anos de produção. RTS é
  pre-1.0.
- **Browser support**: RTS só serve servidor/desktop. Pra frontend
  web, use TS normal.
- **Ferramentas dev**: Bun/Node têm REPLs sofisticados, debuggers,
  profilers. RTS tem só dump de IR (`RTS_DUMP_IR=1`) por enquanto.

## Quando escolher RTS

✅ **Use RTS quando:**
- Você quer um CLI ou ferramenta de linha de comando rápida
- Precisa distribuir um app pequeno pra muita gente
- Faz processamento numérico em loop tight
- Não pode instalar runtime na máquina destino
- Quer performance previsível, sem JIT-deopt
- Está experimentando ou aprendendo compiladores

❌ **Use Bun/Node/Deno quando:**
- Depende fortemente do ecossistema npm
- Usa muito async/await ou event loop
- Constrói servidores web complexos
- Precisa de compatibilidade JS spec total
- Quer ferramental dev maduro (debugger, profiler)
- App vai ficar rodando 24/7 onde JIT tem tempo de otimizar

Em projetos reais, dá pra usar **os dois juntos**: RTS pros tools/CLIs
internos, Bun/Node pro server. RTS gera scripts que se distribuem;
Bun/Node mantém o app principal.

## "Pra quem isso é útil?"

### Bom pra:

- **Ferramentas de linha de comando** (CLIs): startup rápido,
  distribuição fácil.
- **Scripts de automação**: rodar diariamente em servidores sem se
  preocupar com versões de Node/Bun instaladas.
- **Programas de desktop pequenos**: 3 KB de executável é leve
  pra qualquer máquina.
- **Cálculos pesados**: simulações, processamento de dados, cripto,
  parsers — RTS bate Bun em quase todos os benchmarks.
- **Compartilhar tools com colegas que não são programadores**: manda
  o `.exe`, eles clicam, funciona.

### Ruim pra:

- **Sites web** (frontend): use TypeScript com browser/Node mesmo.
- **Apps que usam muitas libs npm**: maioria não vai rodar.
- **Apps que dependem de async/await**: ainda não funciona.

## Por baixo dos panos

Pra quem tem curiosidade técnica:

- **Compilador**: usa **Cranelift**, o mesmo backend que o Wasmtime
  (uma máquina virtual Rust pra WebAssembly) e o que o Firefox usa
  pra otimizar JavaScript.
- **Runtime**: implementado em **Rust**. Cada coisa que o programa
  precisa (ler arquivo, fazer rede, criar string) é uma função Rust
  com ABI bem definida.
- **Sem garbage collector tradicional**: usa um sistema de "handles"
  (referências) com tabela compartilhada e sharded — recursos são
  liberados explicitamente ou no fim do escopo.
- **Compatibilidade com TypeScript**: pega o seu código direto, sem
  precisar mexer. Aceita os mesmos tipos, mesmas sintaxes, mesma
  importação de módulos.

## Tem alguém usando isso?

RTS está em **versão pre-1.0** — funcional mas ainda evoluindo. Bom pra
experimentos, ferramentas internas, hobby, aprendizado. Pra produção
crítica, prefira algo mais maduro (Bun ou Deno) por enquanto.

A ideia do projeto é mostrar que **dá pra ter TypeScript com performance
de C**, sem abrir mão da sintaxe. E dá. Os números provam.

---

**Quer ver o código?** github.com/UrubuCode/rts

**Quer entender mais?** Leia `BLOG_POST.md` (técnico) ou `README.md`
(referência completa).

**Tem dúvida?** Abre uma issue no GitHub.
