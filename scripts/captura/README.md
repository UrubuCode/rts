# Captura da janela

Corre um programa `.ts` no `ui_fixture`, espera que a janela abra e assente,
fotografa-a num PNG e mata o processo.

```bash
bash scripts/captura/janela.sh examples/claude-page-janela.ts saida.png '*wikipedia*'
```

```powershell
powershell -File scripts/captura/janela.ps1 `
  -Programa examples/claude-page-janela.ts -Saida saida.png -Titulo '*wikipedia*' `
  -Ambiente RTS_DOM_PAINT=1
```

Precisa do binário:

```bash
cargo build --release -p rts-host --features ui --example ui_fixture
```

## O que a captura prova, e o que não prova

**Prova que os pixels chegaram ao ecrã.** Que a janela abriu, que o frame foi
tesselado e apresentado, e que o que lá está é o que o motor mostra a alguém que
esteja sentado à frente do computador. É a única pergunta que nem o layout, nem a
lista de pintura, nem as métricas respondem — e foi a que faltou no dia em que
uma página real saiu **branca** com as três a dizerem que estava tudo certo: a
lista trazia oito caixas de `<input>` opacas do tamanho da página, pintadas por
cima de tudo, e só olhando para os pixels é que isso aparece.

**Não prova que está no sítio certo.** A captura não sabe onde um browser
poria cada caixa; uma página inteiramente torta fotografa-se tão bem como uma
correta. Essa pergunta é a da PARIDADE — `bash scripts/parity/run.sh`, que compara
com o Chrome elemento a elemento — e a dos fixtures de CSS,
`bash scripts/css_fixtures.sh`. Use as três: a paridade diz onde as caixas estão,
os fixtures dizem se uma regra de CSS é obedecida, e a captura diz se alguma coisa
chegou a ser desenhada.

**Não é um teste.** Não há valor esperado: quem julga o PNG é quem o abre. Anexá-lo
a um relatório é o uso — "a página aparece" deixa de ser uma afirmação e passa a
ser uma imagem.

## Um antes/depois

`-Exe` aponta para outro `ui_fixture.exe`. É o que permite fotografar um motor
que não é o de agora — construa o commit antigo num worktree, com um `--target-dir`
próprio, e fotografe os dois com o mesmo programa `.ts` e a mesma página em disco,
de modo que a ÚNICA coisa que muda entre as duas imagens seja o motor:

```bash
git worktree add ../rts-antes <commit>
cd ../rts-antes && cargo build --release -p rts-host --features ui \
  --example ui_fixture --target-dir ../rts-antes/target
cd -
powershell -File scripts/captura/janela.ps1 -Programa examples/x.ts `
  -Saida antes.png -Exe ../rts-antes/target/release/examples/ui_fixture.exe
```

Um antes/depois assim é uma MEDIÇÃO, não uma ilustração: as duas capturas
respondem à mesma pergunta com o mesmo input. Uma captura antiga guardada de
memória não serve para isso — nada garante que a página em disco era a mesma.

## As imagens em `out/`, e de onde veio cada uma

`out/` está no `.gitignore` — o que segue é o que uma sessão produz, e a
PROVENIÊNCIA de cada imagem faz parte do que ela vale. Uma captura do script e uma
captura tirada à mão não são a mesma prova: a do script correu um binário
identificado sobre uma entrada conhecida e esperou o assentamento; a manual valia
no momento em que foi tirada e não se repete.

| ficheiro | motor | como foi tirada |
|---|---|---|
| `wikipedia-01-antes-branco.png` | `c54cf6e3`, construído num worktree | pelo script, com `-Exe` |
| `wikipedia-01b-com-quadrados.png` | o de 2026-08-18 a meio da sessão | À MÃO, em PowerShell escrito na altura |
| `wikipedia-02-depois.png` | o da árvore de trabalho | pelo script |
| `minima.png` | o da árvore de trabalho | pelo script (`janela.sh`) |

## Detalhes que importam

- **`PrintWindow` com `PW_RENDERFULLCONTENT`.** É a flag que fotografa uma janela
  desenhada pela GPU; sem ela a superfície wgpu sai preta — uma captura que mente
  em vez de falhar. Também dispensa trazer a janela para a frente, o que evita
  disputar o ecrã com quem esteja a usar o computador.
- **Espera pelo ASSENTAMENTO, não por um tempo fixo.** O script fotografa em
  intervalos e para quando dois retratos seguidos são iguais byte a byte. Uma
  página real leva frames a chegar ao estado final; um `Start-Sleep` arbitrário ou
  era curto de mais (e fotografava o meio do caminho) ou desperdiçava a diferença.
  Se ao fim de `-Espera` segundos a janela ainda mudar, grava o último retrato e
  **avisa** em vez de fingir que assentou.
- **O processo é morto SEMPRE**, num `finally`: um `ui_fixture` esquecido segura o
  executável e o `cargo build` seguinte falha com `LNK1104`.
- **A saída é um parâmetro.** Duas capturas em paralelo com caminho fixo escrevem
  por cima uma da outra — o mesmo defeito que o harness de paridade teve.
- **O `.ps1` está gravado em UTF-8 COM BOM**, e tem de continuar assim: o Windows
  PowerShell 5.1 lê um ficheiro sem BOM como ANSI e rebenta no parse ao primeiro
  acento. Se um editor o gravar sem BOM, o sintoma é um erro de sintaxe numa linha
  que está correta.
