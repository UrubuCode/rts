//! Lote `flex-baseline-2`: a baseline de um CONTENTOR flex vista de FORA
//! (Flexbox §8.5) — o que alinha um `inline-flex`/flex ANINHADO dentro de um
//! contexto ancestral (outro flex com `align-items:baseline`, ou uma linha de
//! texto comum). `flex_baseline.rs` já resolvia o grupo baseline INTERNO de
//! um contentor; nada usava essa resposta quando o PRÓPRIO contentor era, ele
//! mesmo, o item a posicionar — `linha_ib::ascent_do_item` tratava-o como um
//! bloco genérico (fonte própria), que é a pergunta certa para um bloco e
//! errada para um flex (Flexbox §8.5 pede o grupo baseline da 1ª linha, ou o
//! 1º item em fluxo, nunca a fonte do CONTENTOR).
//!
//! Os 10 reftests do WPT deste lote (`flexbox-baseline-align-self-baseline-
//! {horiz,vert}-001`, `flexbox-baseline-multi-item-{horiz,vert}-001{a,b}`,
//! `flexbox-collapsed-item-baseline-001`, `multiline-reverse-wrap-baseline`,
//! `dynamic-baseline-change{,-nested}`) continuam a falhar na régua de
//! pintura — MEDIDO antes e depois desta mudança, números IDÊNTICOS nos dez
//! — porque cada um está bloqueado por uma causa ALHEIA a este mecanismo,
//! já triada: a REFERÊNCIA dos seis primeiros depende de texto multi-fonte
//! num único fluxo inline (`.flexContainer > * {display:inline}` com
//! `font-size` diferente por filho), que este motor não modela — `runs.rs`
//! não carrega um `font-size` por `InlineRun`, só cor/peso/decoração
//! (verificado com uma sonda: um `<span style="font-size:40px">` dentro de um
//! contentor de 10px pinta a `size:10` na `DisplayList`); o de
//! `flexbox-collapsed-item-baseline-001` já foi medido no Edge E num Chrome
//! real pelo lote `visibility-collapse` — o PRÓPRIO Chrome não implementa
//! essa parte da spec, então fechá-lo divergiria da régua que os outros
//! lotes usam; a referência de `multiline-reverse-wrap-baseline` usa uma
//! `<table>` com `vertical-align:bottom` que o lote `flex-baseline` já
//! apontou como bug de TABELA, não de flex; os dois `dynamic-baseline-
//! change*` dependem de um scrollbar real a reduzir o content-box de um
//! `overflow:auto` — este motor só DESENHA a scrollbar (`docs/ui/html-engine/
//! analises/2026-09-04-auditoria-estrutural/README.md`), não a faz ocupar
//! espaço.
//!
//! **Lote `flex-baseline-3` (2026-09-05) tentou o mesmo mecanismo pelo lado
//! do FLUXO INLINE MISTO** (texto + `<div class=flexContainer>` na mesma
//! linha, o caso dos seis primeiros acima) e reverteu: `linha.rs` posiciona
//! um átomo `AtomicKind::Block` com conteúdo sempre no TOPO da linha (`cy`),
//! nunca na baseline PRÓPRIA (`linha_ib::ascent_do_item`) — parecia a mesma
//! lacuna. MEDIDO: ler a baseline própria aí não moveu NENHUM dos onze (a
//! causa de cada um continua a ser a listada acima, o gap de fonte-por-run
//! incluído) e regrediu SEIS que já passavam
//! (`flex-abspos-inset-nested-001/002`, `flexbox-baseline-multi-line-
//! horiz-002/004`, `flexbox-baseline-single-item-001a/001b` — 393→387/489),
//! porque o mesmo `AtomicKind::Block` também cobre `<span style=background>`
//! e inline-block comum, que dependiam do alinhamento pelo topo. Não se
//! repete sem primeiro isolar o caso FLEX do caso genérico.
//!
//! **O 11º do lote `flex-baseline-3`, `flex-vertical-align-effect`, não é
//! deste mecanismo**: é `display:flex` de bloco (não `inline-flex` numa
//! linha), e a asserção é "`vertical-align` não tem efeito num item flex".
//! `claude-paint-dump` confirma a causa: os dois `<input>` sem `width`
//! medem 4×19 e 0×13 em vez de ~180×19 e ~13×13 — `intrinsic_content_width`
//! (`medida.rs`) não sabe medir um CONTROLO DE FORMULÁRIO (só texto, tabela,
//! flex e bloco por filhos; `replaced_inline_size` também não cobre
//! `input`/`button`/`select`/`textarea`), então a flex base size de
//! `flex-basis:auto` cai a ~0. É um gap de TAMANHO INTRÍNSECO de widget em
//! `flex-basis:auto`/shrink-to-fit, não de baseline — lote próprio.
//!
//! Por isso estes testes fixam o MECANISMO por geometria pura, com fixtures
//! desenhadas para CANCELAR a métrica de fonte na álgebra da spec — a mesma
//! técnica de `flex_baseline_corpus.rs` (`claude-flex-align-baseline.html`):
//! todo texto partilha o MESMO `font-size`/`line-height`, então o termo do
//! ascent intrínseco aparece dos dois lados de cada subtração e o resultado
//! observável é só ARITMÉTICA DE MARGEM — o `ApproxMeasurer` (sem fonte real)
//! dá a mesma resposta que o Blink daria.

use crate::table::tests::{geometria, rect};

#[test]
fn baseline_de_flex_aninhado_propaga_para_o_contentor_exterior() {
    // `#inner` é ele próprio um flex com grupo baseline (`#a`/`#b`, MESMO
    // font-size, só `#a` com margin-top:8) — por Flexbox §8.5 a baseline de
    // `#inner` VISTA DE FORA é a do seu PRÓPRIO grupo (8+intrínseco a partir
    // do topo de `#inner`), não a fonte do `#inner`. Sem este lote,
    // `ascent_do_item` usava a fórmula genérica (fonte do contentor,
    // ascent=intrínseco puro) e `#sib` — que devia descer 8px para partilhar
    // a baseline de `#inner` — ficava em y=0 (verificado: comentando o
    // desvio de `linha_ib::ascent_do_item` este teste falha com
    // `#sib` em (0.0,0.0,…) em vez de (0.0,8.0,…)).
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #outer { display: flex; align-items: baseline; width: 400px; background: #eee; }
  #sib { width: 40px; }
  #inner { display: flex; align-items: baseline; width: 80px; }
  #a { width: 40px; margin-top: 8px; }
  #b { width: 40px; }
</style>
<div id="outer"><div id="sib">x</div><div id="inner"><div id="a">a</div><div id="b">b</div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#a"), (40.0, 8.0, 40.0, 20.0), "grupo interno: #a tem a maior margem, fica no topo do inner");
    assert_eq!(r("#b"), (80.0, 8.0, 40.0, 20.0), "grupo interno: #b desce até à baseline de #a");
    assert_eq!(r("#inner"), (40.0, 0.0, 80.0, 28.0), "inner no topo do outer: é ele quem domina o grupo externo");
    assert_eq!(r("#sib"), (0.0, 8.0, 40.0, 20.0), "sib desce 8px para partilhar a baseline do GRUPO de #inner");
}

#[test]
fn sem_participante_interno_a_baseline_do_contentor_e_a_do_primeiro_item() {
    // `#inner2` não declara `align-items:baseline` (fica no default
    // `stretch`) — Flexbox §8.5: sem nenhum item a participar, a baseline do
    // contentor é a do seu PRIMEIRO item em fluxo (`#x1`, margin-top:12),
    // não um grupo. `#x2` (sem margem, mais alto por causa do stretch) NÃO
    // deve influenciar a resposta.
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #outer2 { display: flex; align-items: baseline; width: 400px; background: #eee; }
  #sib2 { width: 40px; }
  #inner2 { display: flex; width: 80px; height: 60px; }
  #x1 { width: 40px; margin-top: 12px; }
  #x2 { width: 40px; }
</style>
<div id="outer2"><div id="sib2">x</div><div id="inner2"><div id="x1">a</div><div id="x2">b</div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#inner2"), (40.0, 0.0, 80.0, 60.0), "inner2 domina o grupo externo (é o único com margem no 1º item)");
    assert_eq!(r("#sib2"), (0.0, 12.0, 40.0, 20.0), "sib2 desce a margem do PRIMEIRO item de inner2 (12), não de x2 (0)");
}

#[test]
fn em_coluna_a_baseline_do_contentor_e_a_do_primeiro_item_apesar_de_align_items_baseline() {
    // `#inner3` é `flex-direction:column` com `align-items:baseline` — a
    // baseline de um item de texto normal não é paralela ao eixo principal
    // vertical, então NUNCA participa do grupo (Flexbox §8.5; a mesma leitura
    // que já faz `coluna.rs::align_offset` cair em `FlexStart`
    // internamente). Vista de FORA, a resposta cai na regra 2: o PRIMEIRO
    // item em fluxo (`#y1`), não um grupo — apesar de a propriedade estar
    // declarada.
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #outer3 { display: flex; align-items: baseline; width: 400px; background: #eee; }
  #sib3 { width: 40px; }
  #inner3 { display: flex; flex-direction: column; align-items: baseline; width: 40px; }
  #y1 { margin-top: 6px; }
  #y2 { margin-top: 30px; }
</style>
<div id="outer3"><div id="sib3">x</div><div id="inner3"><div id="y1">a</div><div id="y2">b</div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#sib3"), (0.0, 6.0, 40.0, 20.0), "desce a margem do PRIMEIRO item (#y1: 6), ignora #y2 (30)");
}

#[test]
fn o_primeiro_item_conta_pela_ordem_de_order_nao_pela_ordem_do_dom() {
    // RETRABALHO (2026-09-05, WPT `flex-order-wrap-reverse-baseline`
    // regrediu no fecho conjunto com os lotes `gap`/`min-auto`): `#inner4`
    // não tem `align-items:baseline` nem `flex-wrap` — cai sempre na regra 2
    // (primeiro item), numa única linha. `#p1` é o PRIMEIRO no DOM mas
    // `order:2` (ÚLTIMO na ordem de flex); `#p2` é o SEGUNDO no DOM mas
    // `order:1` (PRIMEIRO). A resposta certa usa `#p2` (margin-top:4);
    // antes deste retrabalho, `ascent_do_contentor` lia `filhos_flex_em_fluxo`
    // pela ordem do DOM crua e usava `#p1` (margin-top:20) — 20 em vez de 4.
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #outer4 { display: flex; align-items: baseline; width: 400px; background: #eee; }
  #sib4 { width: 40px; }
  #inner4 { display: flex; width: 80px; }
  #p1 { width: 40px; order: 2; margin-top: 20px; }
  #p2 { width: 40px; order: 1; margin-top: 4px; }
</style>
<div id="outer4"><div id="sib4">x</div><div id="inner4"><div id="p1">a</div><div id="p2">b</div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#sib4"), (0.0, 4.0, 40.0, 20.0), "usa a margem de #p2 (order:1, PRIMEIRO na ordem de flex), não a de #p1 (order:2, primeiro só no DOM)");
}

#[test]
fn a_primeira_linha_sob_wrap_reverse_e_a_ultima_na_ordem_de_flex() {
    // RETRABALHO (2026-09-05, o mesmo achado do teste acima, na forma
    // ORIGINAL do WPT: `flex-wrap:wrap-reverse` em vez de `order` puro).
    // `#q1`/`#q2` medem 40px cada num `#inner5` de 40px — cada um força a
    // sua PRÓPRIA linha (`linhas_por_largura`); `wrap-reverse` desenha a
    // linha que o documento escreve DEPOIS (a de `#q2`) no INÍCIO do eixo
    // cruzado (Flexbox §8.3), que é a que conta como "primeira" para a
    // baseline (Flexbox §8.5). A resposta certa usa `#q2` (margin-top:10);
    // sem a reversão de linhas, cairia em `#q1` (sem margem, offset 0).
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  #outer5 { display: flex; align-items: baseline; width: 400px; background: #eee; }
  #sib5 { width: 40px; }
  #inner5 { display: flex; flex-wrap: wrap-reverse; width: 40px; }
  #q1 { width: 40px; }
  #q2 { width: 40px; margin-top: 10px; }
</style>
<div id="outer5"><div id="sib5">x</div><div id="inner5"><div id="q1">a</div><div id="q2">b</div></div></div>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#sib5"), (0.0, 10.0, 40.0, 20.0), "usa a margem de #q2 (linha que o wrap-reverse desenha primeiro), não a de #q1 (0)");
}
