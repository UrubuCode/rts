# Dino Runner — o motor de HTML/CSS/DOM a correr um jogo

React 18 real, sem browser, sem canvas: cada coisa que se vê é um `<div>` ou um
`<img>` posicionado pelo `rts-dom` e pintado pelo egui, sessenta vezes por
segundo, numa janela nativa.

```bash
cargo build --release                       # o binário
target/release/rts.exe run examples/dino/claude-dino-janela.ts
```

## Como se joga

| tecla | o que faz |
|---|---|
| **seta para cima** | salta — e durante o salto não volta a saltar |
| **seta para baixo** | agacha; **durante o salto**, faz cair mais depressa |
| **clique** | salta também, para quem preferir o rato |

Bater num cacto ou numa ave acaba o jogo; clicar (ou saltar) recomeça. O
recorde fica marcado com `HI` e pisca quando é batido.

Um `.exe` de um ficheiro só:

```bash
cargo build --release -p rts-runtime-jit    # o arquivo contra o qual o AOT linka
target/release/rts.exe compile examples/dino/claude-dino-standalone.ts dino.exe \
    --windows-subsystem windows
```

## A anatomia do loop, que é onde está o motor

O programa é dono do tempo. O motor não corre nada sozinho — exporta três
bombas, e é o `while` do lançador que as chama:

```ts
while (egui.isOpen(win)) {
  if (!egui.pump(win)) break;      // eventos da janela (fechar, redimensionar)
  egui.beginFrame(win);
  egui.render(win, d);             // layout + pintura do documento inteiro
  egui.endFrame(win);
  pumpInputEvents(doc);            // teclado -> keydown/keyup no DOM
  pumpEventCallbacks(doc);         // cliques -> os listeners do React
  pumpTimerCallbacks(doc);         // timers  -> o setInterval do jogo
}
```

A ordem não é decorativa. `egui.endFrame` é onde as teclas do SO entram na fila
do DOM; se `pumpInputEvents` viesse antes, cada tecla chegaria um frame
atrasado. E sem `pumpTimerCallbacks` a página monta, fica bonita e não anda —
sem erro nenhum, porque não há erro nenhum: ninguém pediu ao tempo que passasse.

## Os três ficheiros

| ficheiro | o que é |
|---|---|
| `claude-dino.html` | a página: os bundles UMD do React, a tabela de sprites, e o jogo |
| `claude-dino-janela.ts` | o lançador — lê a página do disco, abre a janela, corre o loop |
| `claude-dino-standalone.ts` | o mesmo com a página embutida como literal, para o `.exe` |

Os sprites são gerados por `scripts/claude_dino_sprites.py`, que escreve PNGs
como `data:` URLs sem nenhuma dependência de Python — trinta linhas de `zlib` e
`struct`. Regerar:

```bash
python scripts/claude_dino_sprites.py > sprites.json
```

## Como o jogo está construído por dentro

Duas decisões, e as duas saem das restrições do motor listadas a seguir:

**Uma piscina fixa de `<img>`.** Todas as poses possíveis — as duas do dino a
correr, o agachado, o morto, cada tipo de cacto, as duas asas da ave, as nuvens,
os tijolos do chão, a lua, as estrelas — existem no DOM desde o primeiro render
e nunca são destruídas. Animar é ligar e desligar `display`; mover é mudar
`left`/`top`. Setenta e uma imagens, decodificadas uma vez.

**O estado do jogo não está no React.** Vive numa `useRef` mutável — posição,
velocidade, obstáculos, pontuação — e o React só recebe **um** `setState` por
tique, um contador que serve para pedir um re-render. Um `useState` por grandeza
e um array de obstáculos novo a cada tique seriam milhares de objetos por
segundo, e esta plataforma cobra isso.

## O que este exemplo existe para demonstrar

Um motor de layout prova-se com uma página parada; prova-se **melhor** com uma
que muda sessenta vezes por segundo e reage ao teclado. Foi a escrever isto que
apareceu o defeito do coletor que o PR #2717 corrige: qualquer página com um
relógio e um `addEventListener` perdia os seus handlers ao fim de segundos, com
um `TypeError: object is not a function` que não apontava para lado nenhum.

## As quatro restrições do motor que desenharam este código

Nenhuma delas é preferência de estilo. Cada uma é uma coisa que o motor faz ou
não faz, medida nesta sessão, e mudá-la parte o jogo:

**1. Os pixéis são decodificados por `loadResources`, e só nele.** Ele corre
normalmente uma vez, ANTES dos `<script>` — e as `<img>` deste jogo são criadas
pelo React, portanto depois. O lançador chama-o à mão, e só depois de bombear o
agendador do React (que é *concurrent*: `render` agenda, não desenha). Medido:
antes da chamada, `imageNaturalWidth` de uma `<img>` criada em runtime responde
`0`; depois, responde o tamanho real.

**2. Trocar `.src` depois disso não é caro — é um no-op silencioso.** A imagem
fica congelada na do primeiro decode, sem erro. Por isso o jogo tem uma
**piscina fixa** de `<img>`, uma por pose possível, montada no primeiro render e
nunca destruída: a animação é `display` ligado e desligado, nunca um `src` novo.

**3. `import` num `<script>` de página é `SyntaxError`.** O script é embrulhado
em `function __rts_script() { … }` antes de compilar, e um `import` dentro do
corpo de uma função não existe. O efeito é o pior possível: o script inteiro
falha a compilar, `__runScriptAt` imprime um `console.error` e mais nada
acontece — nem o React monta. É por isso que a decodificação vive no lançador,
que é um `.ts` de topo a sério.

**4. O motor não corre loop nenhum.** Exporta `pumpInputEvents`,
`pumpEventCallbacks` e `pumpTimerCallbacks`, e quem os chama é o programa. O
`while` do lançador é do programa, não do motor — e é `pumpTimerCallbacks` que
faz o `setInterval` do jogo andar.

E uma regra que não é do motor mas da plataforma: **alocar pouco por frame**. O
estado inteiro do jogo vive numa `useRef` mutável e há exactamente um `setState`
por tique — um contador que só serve para pedir um re-render.

## Como foi escrito

Onze agentes: quatro a levantar o que o motor faz mesmo (o caminho do teclado, o
CSS que é honrado, como se comporta uma `<img>`, a mecânica do jogo original),
três a escrever o jogo inteiro de forma independente com prioridades diferentes
— fidelidade, tacto, robustez —, três a julgá-los com lentes distintas (o
motor, o jogador, o adversário), e um a sintetizar.

Ganhou o do tacto, por dois votos a um. Os juízes apontaram dezasseis defeitos
reais, e os dois mais interessantes só apareceram porque alguém fez as contas em
vez de ler: uma fórmula de espaçamento que permitia morte inevitável à
velocidade máxima, e uma ave descrita pelo próprio autor como "obriga a agachar"
que, pelos números da caixa de colisão dele, não obrigava.
