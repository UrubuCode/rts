# Medir uma fixture CSS no Chrome

O `.esperado.json` ao lado de cada fixture é **medido no Chrome**, nunca escrito
à mão. Este ficheiro é o procedimento; o porquê está em `tests/css/README.md`.

## O que é preciso

- `bun` (para o servidor estático)
- o MCP `chrome-devtools` ligado (ferramentas `new_page`, `navigate_page`,
  `resize_page`, `evaluate_script`)

## Os passos

**1. Servir `tests/css/` por HTTP.**

```bash
bun scripts/css_fixtures_serve.ts     # http://127.0.0.1:8731
```

Por HTTP e não por `file://` porque duas páginas `file://` são origens opacas
distintas para o Chrome, e o harness precisa de ler o `contentDocument` do
iframe. Sem isso, a alternativa é uma navegação e uma avaliação POR FIXTURE —
mais de setenta chamadas em vez de duas, e cada uma um sítio a mais onde uma
medição falha em silêncio.

**2. Um palco de 1280x800.** Grave em `tests/css/__harness.tmp.html`:

```html
<!DOCTYPE html><html><head><style>html,body{margin:0;padding:0;overflow:hidden}
iframe{width:1280px;height:800px;border:0;display:block}</style></head>
<body><iframe id="palco"></iframe></body></html>
```

Abra-o (`new_page`) e ponha o separador a **1280x800** com `resize_page`.

**3. Colher.** `evaluate_script`, com `filePath` a apontar para
`tests/css/__medidas.tmp.json`, correndo a função que está em
`examples/claude-css-runner.ts`… não: a função de colheita vive AQUI, porque é
código do Chrome e não do nosso motor:

```js
async () => {
  const PROPS = ["display","position","color","background-color","opacity","visibility",
    "z-index","font-size","line-height","text-align","white-space","letter-spacing",
    "overflow","box-sizing","float","clear","flex-direction","flex-wrap",
    "justify-content","align-items","grid-template-columns","grid-template-rows","gap"];
  const nomes = await (await fetch("/lista")).json();
  const palco = document.getElementById("palco");
  const saida = {}; const problemas = [];
  for (const nome of nomes) {
    if (nome.startsWith("__")) continue;
    await new Promise((ok, falha) => {
      palco.onload = ok; palco.onerror = () => falha(new Error("onerror " + nome));
      palco.src = "/" + nome;
    });
    const d = palco.contentDocument;
    if (d.defaultView.innerWidth !== 1280 || d.defaultView.innerHeight !== 800)
      problemas.push(nome + ": viewport");
    if (d.documentElement.scrollHeight > 800)
      problemas.push(nome + ": transborda");   // a barra de scroll estreitaria o layout
    const caixas = {};
    for (const el of d.querySelectorAll("[id]")) {
      const r = el.getBoundingClientRect();
      const cs = d.defaultView.getComputedStyle(el);
      const estilo = {};
      for (const p of PROPS) estilo[p] = cs.getPropertyValue(p);
      caixas[el.id] = {
        rect: [Math.round(r.x*100)/100, Math.round(r.y*100)/100,
               Math.round(r.width*100)/100, Math.round(r.height*100)/100],
        estilo,
      };
    }
    saida[nome] = { elementos: caixas };
  }
  return { medidas: saida, pedidas: nomes.filter(n => !n.startsWith("__")).length,
           medidas_n: Object.keys(saida).length, problemas };
}
```

**`pedidas` tem de ser igual a `medidas_n`, e `problemas` tem de vir vazio.**
São as duas perguntas sobre a ENTRADA: se cinco fixtures não chegaram a ser
medidas, o denominador é o das que existem e não o das que correram, e uma
barra de scroll no iframe estreita o layout em ~15px e falsifica todas as
larguras percentuais dessa página.

**4. Repartir por fixture.**

```bash
python -c "
import json, io
d=json.load(open('tests/css/__medidas.tmp.json'))
assert d['pedidas']==d['medidas_n'] and not d['problemas'], d['problemas']
for nome,v in d['medidas'].items():
    json.dump({'fixture':nome,
               'regua':'Chrome, via o MCP chrome-devtools, num iframe de 1280x800',
               'viewport':[1280,800],'medido_em':'AAAA-MM-DD','elementos':v['elementos']},
              io.open('tests/css/'+nome[:-5]+'.esperado.json','w',encoding='utf-8'),
              ensure_ascii=False, indent=1, sort_keys=True)
"
rm -f tests/css/__harness.tmp.html tests/css/__medidas.tmp.json
```

**5. Correr o corpus** e ver o que mudou: `bash scripts/css_fixtures.sh`.

## Quando se re-mede

Sempre que o HTML de uma fixture mudar — mesmo que a mudança pareça não ter
efeito de layout, como acrescentar um `<meta>`. Um esperado que já não
corresponde ao ficheiro em disco é a forma mais barata de um corpus mentir.

**Nunca** se re-mede para "consertar" um desvio. O Chrome é a régua; o desvio é
o resultado.

## Sem o MCP e sem o Chrome: o Edge por CDP

Esta máquina não tem o Chrome instalado nem o MCP a funcionar sem ele, e o
Edge é o mesmo Blink. `scripts/css_fixtures_medir_edge.mjs` faz os passos 2 e
3 sozinho — lança o Edge headless, o palco de 1280x800, a mesma função de
colheita — e escreve um JSON com todas as medições:

```bash
bun scripts/css_fixtures_serve.ts &
bun scripts/css_fixtures_medir_edge.mjs medidas.json
```

**O instrumento foi validado antes de contar** (2026-09-04): as 49 fixtures
com esperado medido no Chrome, re-medidas no Edge 152 — 1 104 números, pior
desvio 0. Gravar um esperado novo a partir do JSON é um passo à parte e só
para fixtures SEM esperado; um que já existe nunca se regrava para o número
subir.

