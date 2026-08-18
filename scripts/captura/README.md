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
