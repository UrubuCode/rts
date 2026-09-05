# Dino Runner — o motor de HTML/CSS/DOM a correr um jogo

React 18 real, sem browser, sem canvas: cada coisa que se vê é um `<div>` ou um
`<img>` posicionado pelo `rts-dom` e pintado pelo egui, sessenta vezes por
segundo, numa janela nativa.

```bash
cargo build --release                       # o binário
target/release/rts.exe run examples/dino/claude-dino-janela.ts
```

Setas para cima e baixo, ou o rato. Um `.exe` de um ficheiro só:

```bash
cargo build --release -p rts-runtime-jit    # o arquivo contra o qual o AOT linka
target/release/rts.exe compile examples/dino/claude-dino-standalone.ts dino.exe \
    --windows-subsystem windows
```

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
