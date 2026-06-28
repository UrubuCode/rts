# Avaliação: processador CSS para o rts-dom (LightningCSS vs alternativas)

Experimento + comparação (branch experiment/lightningcss, 2026-06-28). O outro dev
sugeriu lightningcss. Avaliamos NA PRÁTICA (build real) + comparamos as alternativas.

## Medições reais

- **lightningcss adicionado ao rts-dom**: compila, mas traz **~29 deps diretas / ~50
  transitivas** (cssparser, parcel_selectors, phf, sha2, fancy-regex, getrandom...)
  num crate que tinha **0 deps de terceiros**. Pacote monolítico — não dá pegar só o
  parser de seletor.
- O parse é excelente (seletores compostos com specificity calculada, todas as cores/
  unidades/calc()). Confirmou que nosso `green=rgb(0,128,0)` estava certo.

## O ponto DECISIVO

O que FALTA no nosso motor é **#1752: casar seletor composto CONTRA A ÁRVORE**
(`div.card > p` → quais elementos batem). O lightningcss **parseia** o seletor mas
**NÃO faz o match contra um DOM vivo** — é um parser/transformer/minifier de FOLHA de
estilo (build-time), não um motor de estilo. Logo ele NÃO resolve o #1752; só reforça
o parse que já fazemos à mão (0 deps, validado no Chrome em ~116 testes).

## Comparação das crates (para o NOSSO caso)

| Opção | Deps | Faz o match contra a árvore? | Veredito |
|---|---|---|---|
| **À mão** | **0** | ✅ (estender o Selector/selector_matches que já temos) | ⭐ recomendado |
| `simplecss` | 1 (log) | ✅ compostos + descendente/`>`/`+` (sem `~`, sem :nth-child) | atalho leve |
| `selectors`/`parcel_selectors` | ~8 | ✅ tudo, grau-Firefox (trait Element 15+ métodos) | só se precisar `~`/:nth-child |
| **lightningcss** | ~29-50 | ❌ NÃO (só parseia o seletor) | ⛔ errado p/ nós |
| `stylo`/`style` | enorme | ✅ mas acoplado ao Gecko/nightly | inviável |
| `csscascade` | médio | ✅ pilha toda | v0.0.0, imaturo demais |

Complementares (não p/ #1752): `taffy` (só layout flex/grid — poderia validar/estender
nosso layout, útil p/ Grid no futuro); `csscolorparser` (só cores — já temos).

## Recomendação

**Fazer #1752 À MÃO.** Nosso style.rs já tem o `enum Selector`, o `selector_matches`
e a cascade por especificidade. O salto é pequeno: generalizar o Selector para
compostos+combinadores e fazer o matcher andar pelos pais/irmãos do Dom (que já expõe
parent/children/siblings/tag/id/class). ~1 arquivo, 0 deps novas — alinhado ao lema
"menos trabalho/menos deps". Se não quiser escrever o matcher, `simplecss` (1 dep) é o
atalho. lightningcss e stylo: NÃO, em nenhum cenário.

O gargalo do motor NÃO é parse — é o match-de-seletor + cascade + layout, que é nosso.

---

## Adendo: SWC (swc_css) — sugestão do outro dev (medido com cargo)

O outro dev sugeriu o `swc_css` (o parser CSS do SWC). Ponto VÁLIDO: como o RTS já usa
`swc_common`/`swc_ecma_parser` (parser TS/JS) no workspace, o `swc_css` reaproveita a base.

**Deps novas LÍQUIDAS (medido com cargo + diff do Cargo.lock):**
- `swc_css`: **9 crates novas** (swc_css_parser, swc_css_ast + 7 lexical*), todas leves.
  ~88% das deps já estão no workspace via swc_ecma_*. Mantido (v23, mai/2026; o que está
  morto é o npm @swc/css, não os crates Rust).
- `lightningcss`: **37 crates novas** (cssparser, parcel_selectors, stack rkyv 12 crates,
  simd...). Não compartilha nada com o workspace.
- À mão: **0**.

**MAS — o mesmo limite do lightningcss:** swc_css é SÓ parser+transform+codegen.
**NÃO faz o match de seletor contra a árvore** (sem trait Element, sem descer combinadores,
sem especificidade-por-elemento). Confirmado em 3 fontes. O gargalo do #1752 é o MATCH, que
é NOSSO de qualquer jeito — nem swc_css nem lightningcss o fazem. Quem faz é o crate
`selectors` da Mozilla.

**Conclusão (2 metas distintas):**
- **Fechar #1752 (o match):** fazer À MÃO (0 deps; o matcher é nosso com qualquer parser).
- **Modernizar o PARSER de CSS (meta separada, futura):** aí `swc_css` é a melhor escolha
  de lib — 4× menos deps que lightningcss, reusa a base SWC, mesma família. Melhor que
  lightningcss em todos os eixos. Mas isso é "trocar parser", não "resolver #1752".
