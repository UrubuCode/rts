//! AUDITORIA de consistência da árvore — a pergunta que nenhum número de
//! desempenho responde: o que foi medido estava CERTO?
//!
//! Um índice `.classe` apontando para um nó já desligado não fica lento; fica
//! errado, e o sintoma aparece longe da causa (um `querySelector` devolve um nó
//! que não está na página). O mesmo vale para o estado derivado por nó
//! (listeners, valores de input, transições): quando um nó sai da árvore e a
//! entrada fica, o vazamento é silencioso até alguém reusar o índice da arena.
//!
//! Esta varredura é O(n) e **não** é instrumentação: roda sob demanda, sem a
//! feature `metrics`, sobre uma árvore parada. Serve em teste, no harness de
//! métricas e depois de qualquer mudança na invalidação — é ela que separa
//! "ficou mais rápido" de "parou de fazer o trabalho".

use crate::dom::{Dom, NodeIdx, NodeKind};

/// A gravidade de um achado. A distinção existe porque as três exigem ações
/// diferentes, e tratá-las como uma só é o que faz um relatório de consistência
/// virar ruído que ninguém lê.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Invariante do motor violada — a árvore está inconsistente consigo mesma e
    /// alguma consulta PODE responder errado.
    Bug,
    /// Estado derivado que sobreviveu ao nó: por DESIGN as consultas filtram por
    /// `is_attached` (o índice é um superconjunto, não a verdade), então nada
    /// responde errado — mas a entrada nunca é recolhida, e uma página que
    /// remove 10 000 nós carrega 10 000 entradas mortas até trocar de árvore.
    /// É a diferença entre "está errado" e "cresce sem limite".
    Leak,
    /// A PÁGINA faz algo malformado ou ambíguo (dois `id="x"`); o motor tolera, e
    /// quem corrige é o autor do documento.
    Page,
}

/// Um achado da auditoria.
#[derive(Clone, Debug)]
pub struct Finding {
    pub severity: Severity,
    /// Categoria curta e estável — é por ela que um baseline compara.
    pub kind: &'static str,
    /// O nó envolvido, quando há um.
    pub node: Option<NodeIdx>,
    pub detail: String,
}

/// Estatísticas de FORMA da árvore. Não são erros: são o denominador de todo o
/// resto (4000 cascades é muito ou pouco? depende de quantos elementos há) e o
/// que revela uma árvore patológica — profunda demais, ou com um nó de 20 000
/// filhos que faz toda inserção varrer um `Vec` gigante.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    pub nodes: usize,
    pub elements: usize,
    pub text_nodes: usize,
    pub comments: usize,
    pub depth_max: usize,
    pub fanout_max: usize,
    pub fanout_max_node: Option<NodeIdx>,
    pub attrs: usize,
    pub distinct_tags: usize,
    pub distinct_classes: usize,
    pub ids: usize,
    /// Nós presentes na arena e INALCANÇÁVEIS a partir da raiz — memória viva
    /// que nada pode alcançar (o DOM não compacta a arena; um `createElement`
    /// nunca anexado fica aqui, o que é legítimo, e é por isso que este número é
    /// forma e não achado).
    pub unreachable: usize,
}

/// O resultado completo de uma auditoria.
#[derive(Clone, Debug, Default)]
pub struct AuditReport {
    pub shape: Shape,
    pub findings: Vec<Finding>,
}

impl AuditReport {
    pub fn bugs(&self) -> usize {
        self.findings.iter().filter(|f| f.severity == Severity::Bug).count()
    }

    pub fn leaks(&self) -> usize {
        self.findings.iter().filter(|f| f.severity == Severity::Leak).count()
    }

    pub fn page_issues(&self) -> usize {
        self.findings.iter().filter(|f| f.severity == Severity::Page).count()
    }

    /// Relatório legível. Achados iguais são agrupados por categoria com um
    /// exemplo: uma página com 300 ids duplicados deve render três linhas, não
    /// trezentas.
    pub fn report(&self) -> String {
        let s = &self.shape;
        let mut out = format!(
            "    forma: {} nós ({} elementos, {} texto, {} comentários), \
             profundidade {}, fan-out máx {}\n           \
             {} atributos, {} tags distintas, {} classes distintas, {} ids, {} inalcançáveis\n",
            s.nodes,
            s.elements,
            s.text_nodes,
            s.comments,
            s.depth_max,
            s.fanout_max,
            s.attrs,
            s.distinct_tags,
            s.distinct_classes,
            s.ids,
            s.unreachable,
        );
        if self.findings.is_empty() {
            out.push_str("    consistência: OK (nenhum achado)\n");
            return out;
        }
        let mut kinds: Vec<(&'static str, Severity, usize, String)> = Vec::new();
        for f in &self.findings {
            match kinds.iter_mut().find(|(k, _, _, _)| *k == f.kind) {
                Some((_, _, n, _)) => *n += 1,
                None => kinds.push((f.kind, f.severity, 1, f.detail.clone())),
            }
        }
        for (kind, sev, n, example) in kinds {
            let tag = match sev {
                Severity::Bug => "BUG   ",
                Severity::Leak => "vaza  ",
                Severity::Page => "página",
            };
            out.push_str(&format!("    {tag} {kind} ×{n} — ex.: {example}\n"));
        }
        out
    }
}

/// Varre a árvore e confronta cada invariante. O(n) sobre a arena, mais O(total
/// de entradas) sobre os índices.
pub fn audit(dom: &Dom) -> AuditReport {
    let mut r = AuditReport::default();
    let n = dom.nodes.len();
    r.shape.nodes = n;

    // ── 1. Travessia a partir da RAIZ: alcançabilidade, profundidade, ciclos ──
    let mut reached = vec![false; n];
    let mut stack = vec![(dom.root, 0usize)];
    let mut guard = 0usize;
    while let Some((idx, depth)) = stack.pop() {
        guard += 1;
        if guard > n * 4 {
            r.findings.push(Finding {
                severity: Severity::Bug,
                kind: "ciclo-na-arvore",
                node: Some(idx),
                detail: "a travessia visitou mais nós do que a arena tem — há um ciclo".into(),
            });
            break;
        }
        if reached[idx] {
            r.findings.push(Finding {
                severity: Severity::Bug,
                kind: "no-com-dois-pais",
                node: Some(idx),
                detail: format!("nó {idx} alcançado por mais de um caminho"),
            });
            continue;
        }
        reached[idx] = true;
        r.shape.depth_max = r.shape.depth_max.max(depth);
        let node = &dom.nodes[idx];
        if node.children.len() > r.shape.fanout_max {
            r.shape.fanout_max = node.children.len();
            r.shape.fanout_max_node = Some(idx);
        }
        for &c in &node.children {
            if c >= n {
                r.findings.push(Finding {
                    severity: Severity::Bug,
                    kind: "filho-fora-da-arena",
                    node: Some(idx),
                    detail: format!("nó {idx} lista o filho {c}, fora da arena de {n}"),
                });
                continue;
            }
            if dom.nodes[c].parent != Some(idx) {
                r.findings.push(Finding {
                    severity: Severity::Bug,
                    kind: "elo-pai-filho-quebrado",
                    node: Some(c),
                    detail: format!(
                        "nó {c} é filho de {idx} mas seu parent é {:?}",
                        dom.nodes[c].parent
                    ),
                });
            }
            stack.push((c, depth + 1));
        }
    }

    // ── 2. Por nó: tipo, atributos, contagens de forma ───────────────────────
    let mut tags: std::collections::HashSet<&str> = Default::default();
    let mut classes: std::collections::HashSet<String> = Default::default();
    let mut ids_seen: std::collections::HashMap<&str, usize> = Default::default();
    for (idx, node) in dom.nodes.iter().enumerate() {
        if !reached[idx] {
            r.shape.unreachable += 1;
        }
        r.shape.attrs += node.attrs.len();
        match &node.kind {
            NodeKind::Element { tag } => {
                r.shape.elements += 1;
                tags.insert(tag.as_str());
                if tag.is_empty() {
                    r.findings.push(Finding {
                        severity: Severity::Bug,
                        kind: "elemento-sem-tag",
                        node: Some(idx),
                        detail: format!("nó {idx} é Element com tag vazia"),
                    });
                }
                if tag.chars().any(|c| c.is_ascii_uppercase()) {
                    r.findings.push(Finding {
                        severity: Severity::Bug,
                        kind: "tag-nao-normalizada",
                        node: Some(idx),
                        detail: format!("tag `{tag}` não está em minúsculas — seletores não casam"),
                    });
                }
            }
            NodeKind::Text(_) => r.shape.text_nodes += 1,
            NodeKind::Comment(_) => r.shape.comments += 1,
            NodeKind::Document => {}
        }
        let is_element = matches!(node.kind, NodeKind::Element { .. });
        if !is_element && !node.attrs.is_empty() {
            r.findings.push(Finding {
                severity: Severity::Bug,
                kind: "atributo-em-nao-elemento",
                node: Some(idx),
                detail: format!("nó {idx} não é elemento e tem {} atributos", node.attrs.len()),
            });
        }
        if matches!(node.kind, NodeKind::Text(_) | NodeKind::Comment(_))
            && !node.children.is_empty()
        {
            r.findings.push(Finding {
                severity: Severity::Bug,
                kind: "folha-com-filhos",
                node: Some(idx),
                detail: format!("nó de texto/comentário {idx} tem {} filhos", node.children.len()),
            });
        }
        for (i, a) in node.attrs.iter().enumerate() {
            if node.attrs[..i].iter().any(|b| b.name == a.name) {
                r.findings.push(Finding {
                    severity: Severity::Page,
                    kind: "atributo-duplicado",
                    node: Some(idx),
                    detail: format!("nó {idx} repete o atributo `{}`", a.name),
                });
            }
            if a.name.chars().any(|c| c.is_ascii_uppercase()) {
                r.findings.push(Finding {
                    severity: Severity::Bug,
                    kind: "atributo-nao-normalizado",
                    node: Some(idx),
                    detail: format!("atributo `{}` não está em minúsculas", a.name),
                });
            }
        }
        if let Some(id) = node.attr("id") {
            if reached[idx] {
                *ids_seen.entry(id).or_default() += 1;
            }
        }
        if let Some(cl) = node.attr("class") {
            for c in cl.split_whitespace() {
                classes.insert(c.to_string());
            }
        }
    }
    r.shape.distinct_tags = tags.len();
    r.shape.distinct_classes = classes.len();
    r.shape.ids = ids_seen.len();
    for (id, count) in ids_seen {
        if count > 1 {
            r.findings.push(Finding {
                severity: Severity::Page,
                kind: "id-duplicado",
                node: None,
                detail: format!("`#{id}` aparece {count} vezes — a consulta devolve o primeiro"),
            });
        }
    }

    // ── 3. Os ÍNDICES contra a árvore, nos dois sentidos ─────────────────────
    let (id_index, class_index) = dom.debug_indices();
    for (key, bucket) in id_index {
        for &idx in bucket {
            if idx >= n {
                r.findings.push(Finding {
                    severity: Severity::Bug,
                    kind: "indice-id-fora-da-arena",
                    node: Some(idx),
                    detail: format!("id_index[{key}] aponta para {idx}, fora da arena"),
                });
                continue;
            }
            if dom.nodes[idx].attr("id") != Some(key.as_str()) {
                r.findings.push(Finding {
                    severity: Severity::Bug,
                    kind: "indice-id-stale",
                    node: Some(idx),
                    detail: format!("id_index[{key}] aponta para {idx}, que já não tem esse id"),
                });
            } else if !reached[idx] {
                r.findings.push(Finding {
                    severity: Severity::Leak,
                    kind: "indice-id-desanexado",
                    node: Some(idx),
                    detail: format!("id_index[{key}] aponta para {idx}, fora da árvore"),
                });
            }
        }
    }
    for (key, bucket) in class_index {
        for &idx in bucket {
            if idx >= n {
                r.findings.push(Finding {
                    severity: Severity::Bug,
                    kind: "indice-classe-fora-da-arena",
                    node: Some(idx),
                    detail: format!("class_index[{key}] aponta para {idx}, fora da arena"),
                });
                continue;
            }
            let has = dom.nodes[idx]
                .attr("class")
                .map(|c| c.split_whitespace().any(|x| x == key))
                .unwrap_or(false);
            if !has {
                r.findings.push(Finding {
                    severity: Severity::Bug,
                    kind: "indice-classe-stale",
                    node: Some(idx),
                    detail: format!("class_index[{key}] aponta para {idx}, que não tem a classe"),
                });
            } else if !reached[idx] {
                r.findings.push(Finding {
                    severity: Severity::Leak,
                    kind: "indice-classe-desanexado",
                    node: Some(idx),
                    detail: format!("class_index[{key}] aponta para {idx}, fora da árvore"),
                });
            }
        }
    }
    // …e o sentido inverso: um nó com id/classe que o índice NÃO conhece é a
    // falha que faz `querySelector` devolver `null` numa página correta.
    for (idx, node) in dom.nodes.iter().enumerate() {
        if !reached[idx] {
            continue;
        }
        if let Some(id) = node.attr("id") {
            if !id_index.get(id).map(|b| b.contains(&idx)).unwrap_or(false) {
                r.findings.push(Finding {
                    severity: Severity::Bug,
                    kind: "no-com-id-fora-do-indice",
                    node: Some(idx),
                    detail: format!("nó {idx} tem id `{id}` e não está no id_index"),
                });
            }
        }
        if let Some(cl) = node.attr("class") {
            for c in cl.split_whitespace() {
                if !class_index.get(c).map(|b| b.contains(&idx)).unwrap_or(false) {
                    r.findings.push(Finding {
                        severity: Severity::Bug,
                        kind: "no-com-classe-fora-do-indice",
                        node: Some(idx),
                        detail: format!("nó {idx} tem a classe `{c}` e não está no class_index"),
                    });
                }
            }
        }
    }

    // ── 4. Estado DERIVADO por nó: entradas órfãs ────────────────────────────
    for (label, idx) in dom.derived_node_state() {
        if idx >= n {
            r.findings.push(Finding {
                severity: Severity::Bug,
                kind: "estado-derivado-fora-da-arena",
                node: Some(idx),
                detail: format!("{label} tem entrada para o nó {idx}, fora da arena de {n}"),
            });
        } else if !reached[idx] {
            r.findings.push(Finding {
                severity: Severity::Leak,
                kind: "estado-derivado-orfao",
                node: Some(idx),
                detail: format!("{label} guarda o nó {idx}, que já não está na árvore"),
            });
        }
    }

    // ── 5. Tabelas paralelas à arena ─────────────────────────────────────────
    if dom.layout_epoch_len() != n {
        r.findings.push(Finding {
            severity: Severity::Bug,
            kind: "tabela-paralela-dessincronizada",
            node: None,
            detail: format!("layout_epochs tem {} entradas para {n} nós", dom.layout_epoch_len()),
        });
    }

    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_html_to_dom;

    /// Uma página bem formada não produz achado NENHUM. Sem isto, a auditoria
    /// que sempre acusa algo vira ruído e ninguém a lê.
    #[test]
    fn pagina_sa_nao_gera_achados() {
        let dom = parse_html_to_dom(
            "<html><body><div id=\"a\" class=\"x y\"><p>oi</p></div><!-- c --></body></html>",
        );
        let r = audit(&dom);
        assert_eq!(r.bugs(), 0, "{}", r.report());
        assert_eq!(r.leaks(), 0, "{}", r.report());
        assert_eq!(r.page_issues(), 0, "{}", r.report());
        assert!(r.shape.elements >= 4);
        assert!(r.shape.depth_max >= 3);
    }

    /// Dois `id` iguais são problema da PÁGINA, não do motor — e a distinção
    /// entre as duas gravidades é o que decide quem tem de consertar.
    #[test]
    fn id_duplicado_e_problema_da_pagina() {
        let dom = parse_html_to_dom("<div id=\"a\"></div><div id=\"a\"></div>");
        let r = audit(&dom);
        assert_eq!(r.bugs(), 0, "{}", r.report());
        assert_eq!(r.page_issues(), 1);
        assert!(r.report().contains("id-duplicado"));
    }

    /// Um nó criado e nunca anexado não é um erro — é forma. Confundir os dois
    /// faria a auditoria acusar todo `createElement` que ainda não foi inserido.
    #[test]
    fn no_criado_e_nao_anexado_conta_como_inalcancavel_sem_ser_bug() {
        let mut dom = parse_html_to_dom("<div id=\"host\"></div>");
        let _solto = dom.create_element("span");
        let r = audit(&dom);
        assert_eq!(r.bugs(), 0, "{}", r.report());
        assert_eq!(r.shape.unreachable, 1);
    }

    /// Remover um nó indexado deixa a entrada no índice — por DESIGN (a consulta
    /// filtra por `is_attached`, o comentário em `query_idx` diz isso). O que a
    /// auditoria pina aqui é a fronteira: nada responde errado (`bugs() == 0`) e
    /// a entrada morta é contada como VAZAMENTO. Se um dia a remoção passar a
    /// deindexar, este teste é o que avisa que a classificação mudou.
    #[test]
    fn remocao_deixa_entrada_de_indice_como_vazamento_nao_como_bug() {
        let mut dom = parse_html_to_dom("<div id=\"a\" class=\"card\"></div><p>x</p>");
        let alvo = dom.query("#a").expect("nó");
        dom.remove_node(alvo);
        let r = audit(&dom);
        assert_eq!(r.bugs(), 0, "{}", r.report());
        assert_eq!(r.leaks(), 2, "id e classe: {}", r.report());
    }
}
