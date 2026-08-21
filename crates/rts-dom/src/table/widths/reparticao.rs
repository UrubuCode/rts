//! A REPARTIÇÃO propriamente dita: dada uma lista de colunas já medidas e a
//! largura disponível, quanto leva cada uma.
//!
//! Está separada da medição (o módulo pai) porque são duas perguntas com fontes
//! diferentes: a medição lê a árvore e o estilo, esta não lê nada — recebe
//! números e devolve números. É o que a torna testável com uma lista escrita à
//! mão, e é onde a repartição por CLASSE vive.

use super::Coluna;

/// Os quatro degraus. Nomes e não números porque as somas são indexadas em
/// muitos sítios e um `soma[2]` não diz qual dos palpites é.
const MINIMO: usize = 0;
const PERCENTAGEM: usize = 1;
const DECLARADO: usize = 2;
const MAXIMO: usize = 3;

/// A largura USADA de cada coluna, dada a largura de conteúdo disponível para as
/// colunas (já sem os `border-spacing`).
///
/// **Não é uma interpolação — é uma ESCADA.** Havia aqui três regimes decididos
/// pela SOMA (não cabe o mínimo / cabe o máximo / entre os dois) e uma
/// distribuição linear da folga de todas as colunas ao mesmo tempo. O Blink
/// escolhe um REGIME e faz crescer nele **uma classe de colunas só**, deixando
/// as outras congeladas no valor que tinham no degrau anterior.
///
/// A diferença não é de afinação, e o sinal dela foi medido: a interpolação dava
/// a maior fatia à coluna que devia estar PARADA. Uma coluna declarada tem
/// máximo grande, e repartir a sobra em proporção ao máximo premiava-a
/// exatamente onde o browser a congela — `[200, 400]` no Chrome saía
/// `[480, 120]` aqui.
///
/// Os quatro degraus, cada um a pergunta "e se":
///
/// | palpite | percentuais | declaradas | auto |
/// |---|---|---|---|
/// | mínimo | mínimo | mínimo | mínimo |
/// | percentagem | a sua % | mínimo | mínimo |
/// | declarado | a sua % | **máximo** | mínimo |
/// | máximo | a sua % | máximo | **máximo** |
///
/// Escolhe-se o PRIMEIRO cuja soma já chega ao alvo, e o excedente vai só para a
/// classe que cresce NESSE degrau, em proporção ao aumento que ela própria lhe
/// deu — não à folga, que é de todas. Acima do último degrau há três ramos, por
/// ordem: crescem as auto; sem auto, as declaradas; sem essas, as percentuais.
pub(crate) fn resolve_colunas(cols: &[Coluna], disponivel: f32) -> Vec<f32> {
    if cols.is_empty() {
        return Vec::new();
    }
    // O alvo nunca fica abaixo da soma dos mínimos: uma tabela não esmaga o
    // conteúdo abaixo do indivisível, transborda. É a mesma linha que o Blink
    // escreve antes de escolher o degrau, e é ela que torna o primeiro degrau
    // um caso normal em vez de um ramo à parte.
    let total_min: f32 = cols.iter().map(|c| c.min).sum();
    let alvo = disponivel.max(total_min);

    let pct = percentagens_normalizadas(cols);
    // A percentagem resolvida contra o alvo, e nunca abaixo do mínimo da coluna:
    // uma percentagem que não chega para o conteúdo é levantada, não respeitada
    // com o texto a transbordar.
    //
    // Sem somar padding nem bordas, e isso foi MEDIDO: o Blink só conta a
    // moldura na percentagem quando a tabela é `fixed`, e esta função serve a
    // auto. Ver a nota em [`super::Coluna`] — o termo chegou a estar aqui.
    let resolvida =
        |i: usize| -> f32 { (pct[i].unwrap_or(0.0) * alvo / 100.0).max(cols[i].min) };

    // As quatro somas hipotéticas, e o AUMENTO que cada degrau traz sobre o
    // anterior. É esse aumento que reparte o excedente do degrau escolhido.
    let mut soma = [0.0f32; 4];
    let mut aumento = [0.0f32; 4];
    let (mut n_pct, mut n_dec, mut n_auto) = (0usize, 0usize, 0usize);
    let (mut max_auto, mut max_dec, mut pct_total) = (0.0f32, 0.0f32, 0.0f32);
    for (i, c) in cols.iter().enumerate() {
        soma[MINIMO] += c.min;
        if pct[i].is_some() {
            n_pct += 1;
            pct_total += pct[i].unwrap_or(0.0);
            let w = resolvida(i);
            soma[PERCENTAGEM] += w;
            soma[DECLARADO] += w;
            soma[MAXIMO] += w;
            aumento[PERCENTAGEM] += w - c.min;
        } else if c.restringida {
            n_dec += 1;
            max_dec += c.max;
            soma[PERCENTAGEM] += c.min;
            soma[DECLARADO] += c.max;
            soma[MAXIMO] += c.max;
            aumento[DECLARADO] += c.max - c.min;
        } else {
            n_auto += 1;
            max_auto += c.max;
            soma[PERCENTAGEM] += c.min;
            soma[DECLARADO] += c.min;
            soma[MAXIMO] += c.max;
            aumento[MAXIMO] += c.max - c.min;
        }
    }

    let degrau = (MINIMO..=MAXIMO).find(|&i| soma[i] >= alvo);
    // Cada coluna parte do seu mínimo; cada ramo levanta as que lhe pertencem.
    let mut w: Vec<f32> = cols.iter().map(|c| c.min).collect();
    // A última coluna que ficou a crescer: é nela que o défice fecha.
    let mut ultima = None;
    let mut defice;

    match degrau {
        Some(MINIMO) => return w,
        Some(PERCENTAGEM) => {
            defice = alvo - soma[MINIMO];
            let a_dividir = defice;
            for i in 0..cols.len() {
                // As auto e as declaradas ficam no mínimo, que já está em `w`.
                if pct[i].is_none() {
                    continue;
                }
                ultima = Some(i);
                let delta = fatia(
                    a_dividir,
                    resolvida(i) - cols[i].min,
                    aumento[PERCENTAGEM],
                    n_pct,
                );
                defice -= delta;
                w[i] = cols[i].min + delta;
            }
        }
        Some(DECLARADO) => {
            defice = alvo - soma[PERCENTAGEM];
            let a_dividir = defice;
            for i in 0..cols.len() {
                if pct[i].is_some() {
                    w[i] = resolvida(i);
                } else if cols[i].restringida {
                    ultima = Some(i);
                    let delta =
                        fatia(a_dividir, cols[i].max - cols[i].min, aumento[DECLARADO], n_dec);
                    defice -= delta;
                    w[i] = cols[i].min + delta;
                }
            }
        }
        Some(MAXIMO) => {
            defice = alvo - soma[DECLARADO];
            let a_dividir = defice;
            // O CASO EXATO: quando o alvo bate na soma dos máximos — o normal de
            // uma tabela sem `width`, que é o caso mais comum de uma página —
            // cada coluna recebe LITERALMENTE o seu máximo, sem passar pela
            // matemática de distribuição. A razão não é cosmética e está dita na
            // source: o arredondamento tira meio pixel a uma célula, ela quebra
            // uma linha que não devia, e isso muda a ALTURA da linha inteira.
            //
            // O défice tem de ir a zero JUNTO, senão a regra de fechar na última
            // coluna volta a somar exatamente o que isto acabou de evitar.
            let exato = (alvo - soma[MAXIMO]).abs() < 0.001;
            if exato {
                defice = 0.0;
            }
            for i in 0..cols.len() {
                if pct[i].is_some() {
                    w[i] = resolvida(i);
                } else if cols[i].restringida || exato {
                    w[i] = cols[i].max;
                } else {
                    ultima = Some(i);
                    let delta = fatia(a_dividir, cols[i].max - cols[i].min, aumento[MAXIMO], n_auto);
                    defice -= delta;
                    w[i] = cols[i].min + delta;
                }
            }
        }
        _ => {
            // ACIMA DO ÚLTIMO DEGRAU. Só uma classe cresce, e aqui a proporção é
            // sobre o MÁXIMO e não sobre o aumento: a folga já foi toda dada no
            // degrau anterior, e o que se reparte agora é sobra pura.
            defice = alvo - soma[MAXIMO];
            let a_dividir = defice;
            for i in 0..cols.len() {
                w[i] = if pct[i].is_some() {
                    resolvida(i)
                } else {
                    cols[i].max
                };
            }
            if n_auto > 0 {
                for i in 0..cols.len() {
                    if pct[i].is_some() || cols[i].restringida {
                        continue;
                    }
                    ultima = Some(i);
                    let delta = fatia(a_dividir, cols[i].max, max_auto, n_auto);
                    defice -= delta;
                    w[i] = cols[i].max + delta;
                }
            } else if n_dec > 0 {
                // As declaradas só crescem quando o alvo é a largura da TABELA, e
                // não quando é uma célula com `colspan` a pedir espaço. Aqui é
                // sempre o primeiro caso: a repartição do `colspan` não passa por
                // esta função — está em `colunas_min_max`.
                for i in 0..cols.len() {
                    if pct[i].is_some() {
                        continue;
                    }
                    ultima = Some(i);
                    let delta = fatia(a_dividir, cols[i].max, max_dec, n_dec);
                    defice -= delta;
                    w[i] = cols[i].max + delta;
                }
            } else if n_pct > 0 {
                // Só percentuais: cada uma leva a fatia da sua PRÓPRIA
                // percentagem, e não do seu máximo.
                for i in 0..cols.len() {
                    let Some(p) = pct[i] else { continue };
                    ultima = Some(i);
                    let delta = fatia(a_dividir, p, pct_total, n_pct);
                    defice -= delta;
                    w[i] = resolvida(i) + delta;
                }
            }
        }
    }

    // O que sobrou do arredondamento vai INTEIRO para a última que cresceu. Sem
    // isto a soma das colunas não fecha com a largura da tabela e o erro
    // espalha-se por todas em vez de ficar numa.
    if let Some(i) = ultima
        && defice.abs() > 0.0001
    {
        w[i] += defice;
    }
    w
}

/// A fatia de `total` que cabe a quem contribuiu `parte` de `contribuicao`.
///
/// Com contribuição nula reparte-se por IGUAL, e não se devolve zero: é o que o
/// Blink faz, e é o único ramo que não divide por zero quando todas as colunas
/// da classe já estão no seu limite — uma tabela de colunas todas fixas cai
/// exatamente aí.
fn fatia(total: f32, parte: f32, contribuicao: f32, quantas: usize) -> f32 {
    if contribuicao > 0.0 {
        total * parte / contribuicao
    } else if quantas > 0 {
        total / quantas as f32
    } else {
        0.0
    }
}

/// As percentagens já NORMALIZADAS, na ordem das colunas.
///
/// Uma tabela pode pedir mais de 100% — `60%` e `60%` é legal e acontece. Numa
/// tabela AUTO (a que esta função serve; a `fixed` tem o seu próprio caminho) o
/// Blink **corta por ordem de documento**: cada coluna leva no máximo o que
/// falta para 100, portanto `60%+60%` dá 60 e 40. A alternativa — escalar as
/// duas proporcionalmente, que é o que a `fixed` faz — daria 50 e 50.
///
/// Não é uma escolha entre duas leituras da spec: mediu-se no Chrome, e
/// `60%+60%` numa tabela de 600px dá `[360, 240]`. **A assimetria é a assinatura
/// do corte por ordem**, e é o único sinal que distingue os dois ramos — uma
/// escala proporcional daria `[300, 300]`, que era por acaso o que este ficheiro
/// respondia antes, pela razão errada.
fn percentagens_normalizadas(cols: &[Coluna]) -> Vec<Option<f32>> {
    let mut acumulado = 0.0f32;
    cols.iter()
        .map(|c| {
            let p = c.percentagem?;
            let cabe = (100.0 - acumulado).max(0.0);
            let p = p.min(cabe);
            acumulado += p;
            Some(p)
        })
        .collect()
}

/// `table-layout: fixed` — as larguras vêm da PRIMEIRA linha (e dos `<col>`), o
/// conteúdo não é consultado, e o que sobra divide-se igualmente pelas colunas
/// sem largura declarada. É o algoritmo que existe para ser rápido: nenhuma
/// célula é medida.
///
/// `declaradas[i] == None` = coluna sem largura declarada.
pub(crate) fn resolve_fixo(declaradas: &[Option<f32>], disponivel: f32) -> Vec<f32> {
    let soma: f32 = declaradas.iter().flatten().sum();
    let livres = declaradas.iter().filter(|d| d.is_none()).count();
    if livres == 0 {
        // Todas declaradas: escalam para encher a largura da tabela (a spec manda
        // a tabela ficar com a largura pedida, não com a soma das colunas).
        if soma <= 0.0 {
            return vec![0.0; declaradas.len()];
        }
        let k = disponivel / soma;
        return declaradas.iter().map(|d| d.unwrap_or(0.0) * k).collect();
    }
    let quota = ((disponivel - soma) / livres as f32).max(0.0);
    declaradas.iter().map(|d| d.unwrap_or(quota)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::widths::colunas_min_max;

    fn mm(min: f32, max: f32) -> Coluna {
        Coluna {
            min,
            max,
            ..Coluna::default()
        }
    }

    /// Uma coluna declarada: mínimo e máximo iguais não a distinguem de uma auto
    /// satisfeita, é a bandeira que a distingue.
    fn dec(largura: f32) -> Coluna {
        Coluna {
            min: largura,
            max: largura,
            restringida: true,
            ..Coluna::default()
        }
    }

    fn percentual(min: f32, max: f32, p: f32) -> Coluna {
        Coluna {
            min,
            max,
            percentagem: Some(p),
            restringida: true,
        }
    }

    #[test]
    fn a_folga_e_repartida_e_nao_o_maximo() {
        // Uma coluna estreita e satisfeita (10..12) ao lado de uma esfomeada
        // (10..110): 100px de folga total, 40 para dividir. Com as duas na mesma
        // classe, o degrau do máximo reparte pelo aumento — que é a folga.
        let w = resolve_colunas(&[mm(10.0, 12.0), mm(10.0, 110.0)], 60.0);
        // A estreita leva 2% dos 40 (a sua folga é 2 de 102), não 10%.
        assert!((w[0] - 10.78).abs() < 0.05, "coluna estreita = {}", w[0]);
        assert!((w[1] - 49.22).abs() < 0.05, "coluna larga = {}", w[1]);
        assert!((w[0] + w[1] - 60.0).abs() < 0.01);
    }

    #[test]
    fn abaixo_do_minimo_a_tabela_transborda_em_vez_de_esmagar() {
        let w = resolve_colunas(&[mm(50.0, 80.0), mm(50.0, 80.0)], 40.0);
        assert_eq!(w, vec![50.0, 50.0]);
    }

    #[test]
    fn a_declarada_fica_congelada_enquanto_a_auto_cresce() {
        // O degrau do máximo: a declarada já está no seu valor e não recebe nada
        // do excedente. Antes da escada levava a maior fatia, por ter o maior
        // máximo.
        let w = resolve_colunas(&[dec(200.0), mm(50.0, 100.0)], 280.0);
        assert_eq!(w, vec![200.0, 80.0]);
    }

    #[test]
    fn o_defice_do_arredondamento_fecha_numa_coluna_so() {
        // Três auto iguais em 100px: 33,33 cada deixa 0,01 por colocar, e esse
        // resto vai inteiro para a última em vez de se espalhar pelas três.
        let w = resolve_colunas(&[mm(0.0, 10.0), mm(0.0, 10.0), mm(0.0, 10.0)], 100.0);
        assert!(
            (w.iter().sum::<f32>() - 100.0).abs() < 0.0001,
            "a soma tem de fechar com o alvo: {w:?}"
        );
    }

    #[test]
    fn o_caso_exato_da_o_maximo_literal_a_cada_coluna() {
        // O alvo bate na soma dos máximos: nenhuma conta de distribuição corre, e
        // é por isso que uma célula não quebra linha por meio pixel.
        let w = resolve_colunas(&[dec(200.0), mm(50.0, 100.0)], 300.0);
        assert_eq!(w, vec![200.0, 100.0]);
    }

    #[test]
    fn percentagens_acima_de_cem_sao_cortadas_por_ordem_e_nao_escaladas() {
        // 60% + 60% numa tabela de 600: a primeira leva os seus 60%, a segunda
        // leva o que resta. Escalar as duas daria [300, 300].
        let w = resolve_colunas(
            &[percentual(50.0, 50.0, 60.0), percentual(50.0, 50.0, 60.0)],
            600.0,
        );
        assert!((w[0] - 360.0).abs() < 0.01, "primeira = {}", w[0]);
        assert!((w[1] - 240.0).abs() < 0.01, "segunda = {}", w[1]);
    }

    #[test]
    fn acima_do_maximo_so_as_auto_crescem_e_em_proporcao_ao_maximo() {
        // Uma declarada de 200 e duas auto de 100 em 600px: os 200 que sobram
        // dividem-se pelas auto, e a declarada não vê nada deles.
        let w = resolve_colunas(&[dec(200.0), mm(100.0, 100.0), mm(100.0, 100.0)], 600.0);
        assert_eq!(w, vec![200.0, 200.0, 200.0]);
    }

    #[test]
    fn sem_auto_nenhuma_as_declaradas_crescem_em_proporcao_ao_que_declararam() {
        let w = resolve_colunas(&[dec(200.0), dec(300.0)], 800.0);
        assert_eq!(w, vec![320.0, 480.0]);
    }

    #[test]
    fn colspan_so_levanta_colunas_nunca_as_baixa() {
        // Duas colunas já com 100 cada, e um cabeçalho colspan=2 que só quer 50.
        let cols = colunas_min_max(
            &[
                (0, 1, mm(100.0, 100.0)),
                (1, 1, mm(100.0, 100.0)),
                (0, 2, mm(50.0, 50.0)),
            ],
            2,
            0.0,
        );
        assert_eq!(cols[0].min, 100.0);
        assert_eq!(cols[1].min, 100.0);
    }

    #[test]
    fn colspan_maior_que_as_colunas_reparte_o_que_falta_por_igual() {
        let cols = colunas_min_max(&[(0, 2, mm(100.0, 100.0))], 2, 0.0);
        assert_eq!(cols[0].min, 50.0);
        assert_eq!(cols[1].min, 50.0);
    }

    #[test]
    fn fixo_divide_o_resto_pelas_colunas_sem_largura() {
        let w = resolve_fixo(&[Some(100.0), None, None], 300.0);
        assert_eq!(w, vec![100.0, 100.0, 100.0]);
    }

    #[test]
    fn fixo_com_todas_declaradas_escala_para_a_largura_da_tabela() {
        let w = resolve_fixo(&[Some(50.0), Some(50.0)], 200.0);
        assert_eq!(w, vec![100.0, 100.0]);
    }
}
