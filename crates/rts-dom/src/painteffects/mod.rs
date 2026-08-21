//! `filter` e `clip-path` do lado do PRODUTOR de pintura — o que é exprimível
//! numa display list sem grupo de compositing, e o que é recusado por não ser.
//!
//! ## A regra que decide o que entra aqui
//!
//! Uma superfície que não faz o que o nome dela diz não entra. Aplica-se com
//! força a estas duas propriedades porque ambas têm uma aproximação óbvia e
//! errada: um `blur()` que só baixa o alpha, um `polygon()` recortado pelo
//! retângulo envolvente. Nos dois casos o resultado é um desenho ERRADO com
//! aparência de certo — pior do que não pintar nada de diferente, porque quem
//! olha não tem como saber que está a ver uma mentira.
//!
//! Por isso a fronteira é: entra o que se calcula EXATAMENTE, sai o resto, e
//! quem sai deixa o elemento igual ao que estava antes desta propriedade
//! existir.
//!
//! ## `filter`: a metade que é aritmética de cor
//!
//! `brightness`, `contrast`, `invert`, `grayscale`, `sepia`, `saturate`,
//! `hue-rotate` e `opacity` são, todas, uma matriz 3×3 sobre RGB mais um
//! deslocamento — o próprio spec (Filter Effects 1 §8) define-as assim. Sobre
//! uma cor sólida, uma borda, um gradiente ou um texto, aplicar a matriz dá o
//! MESMO pixel que um browser dá: não é aproximação, é a definição.
//!
//! Ficam de fora `blur()` e `drop-shadow()`, que são os dois que precisam do
//! que um motor imediato não tem — o resultado já rasterizado do elemento, para
//! o reprocessar. O egui pinta direto no buffer do frame e não expõe render
//! target por elemento; `epaint::Shadow` desfoca uma sombra de retângulo que ele
//! próprio gera, não conteúdo alheio, e o `drop-shadow` do CSS segue a silhueta
//! ALPHA do elemento, que uma display list de retângulos não conhece. Fazê-los
//! a sério é um pass de wgpu com readback, não uma mudança de display list.
//!
//! **A cadeia é tudo-ou-nada, e é aqui que está a decisão que mais importa.**
//! Perante `filter: blur(4px) brightness(1.2)`, aplicar só o `brightness`
//! daria um elemento nítido e mais claro — que não é nem o pedido nem o
//! anterior, é um terceiro desenho que ninguém escreveu. Uma função não
//! suportada na cadeia recusa a cadeia INTEIRA e o elemento fica intacto.
//!
//! ## `clip-path`: só o retângulo, porque só o retângulo existe
//!
//! O recorte do egui (e o do `BeginClip`) é um retângulo alinhado aos eixos.
//! `inset()` sem `round` é exatamente isso, portanto é exato. `polygon()`,
//! `circle()`, `ellipse()`, `path()` e `inset()` COM raio não são exprimíveis —
//! e a aproximação disponível (recortar pela caixa envolvente) é o caso
//! exemplar do desenho errado com aparência de certo: um losango recortado
//! assim continua a ser um quadrado.

use crate::layout::Rect;

/// Uma cadeia de `filter` já reduzida a UMA transformação de cor.
///
/// Guardar a matriz composta em vez da lista de funções é o que torna o custo
/// independente do tamanho da cadeia: as funções compõem-se uma vez, por
/// elemento, e depois cada cor pintada paga um produto 3×3 — e não uma
/// re-interpretação da string por item de display.
///
/// `m` é linha-a-linha (R, G, B); `o` o deslocamento por canal; `a` o
/// multiplicador de alpha (só o `opacity()` mexe nele — nenhuma das funções
/// suportadas mistura alpha DENTRO do RGB, e é isso que dispensa aqui a matriz
/// 5×4 completa do SVG).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FilterMatriz {
    pub m: [[f32; 3]; 3],
    pub o: [f32; 3],
    pub a: f32,
}

impl FilterMatriz {
    /// A identidade — o que uma cadeia vazia significa.
    pub const IDENTIDADE: FilterMatriz = FilterMatriz {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        o: [0.0, 0.0, 0.0],
        a: 1.0,
    };

    /// Esta matriz não muda nenhuma cor?
    ///
    /// Serve o mesmo papel que o `Corners::any()` do lado dos cantos: uma
    /// pergunta que o produtor faz UMA vez para saber se pode saltar o trabalho
    /// todo. `filter: none`, `filter: brightness(1)` e uma cadeia recusada
    /// respondem todas o mesmo — e as três devem deixar a cor como estava.
    pub fn e_identidade(&self) -> bool {
        *self == FilterMatriz::IDENTIDADE
    }

    /// `self` depois de `outra` — a ordem de aplicação do CSS, que é a de
    /// leitura: em `filter: grayscale(1) invert(1)` o cinzento vem primeiro.
    fn compor(&self, outra: &FilterMatriz) -> FilterMatriz {
        let mut m = [[0.0f32; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] = (0..3).map(|k| outra.m[i][k] * self.m[k][j]).sum();
            }
        }
        let mut o = [0.0f32; 3];
        for i in 0..3 {
            o[i] = (0..3).map(|k| outra.m[i][k] * self.o[k]).sum::<f32>() + outra.o[i];
        }
        FilterMatriz {
            m,
            o,
            a: self.a * outra.a,
        }
    }

    /// Aplica a uma cor `0xRRGGBBAA`.
    ///
    /// Em sRGB direto, sem passar por linear: o `filter` do CSS (o shorthand com
    /// funções, distinto dos primitivos SVG) especifica
    /// `color-interpolation-filters: sRGB`, portanto converter para linear aqui
    /// daria um cinzento diferente do que o browser mostra.
    pub fn aplicar(&self, cor: u32) -> u32 {
        if self.e_identidade() {
            return cor;
        }
        let c = [
            ((cor >> 24) & 0xFF) as f32 / 255.0,
            ((cor >> 16) & 0xFF) as f32 / 255.0,
            ((cor >> 8) & 0xFF) as f32 / 255.0,
        ];
        let mut out = [0u32; 3];
        for i in 0..3 {
            let v = self.m[i][0] * c[0] + self.m[i][1] * c[1] + self.m[i][2] * c[2] + self.o[i];
            out[i] = (v * 255.0).round().clamp(0.0, 255.0) as u32;
        }
        let alpha = (cor & 0xFF) as f32 * self.a;
        let alpha = alpha.round().clamp(0.0, 255.0) as u32;
        (out[0] << 24) | (out[1] << 16) | (out[2] << 8) | alpha
    }

    /// A cor final: o filtro e depois o `opacity` do elemento.
    ///
    /// A ordem é a do CSS — o filtro atua sobre o elemento renderizado e a
    /// opacidade compõe o resultado — e existe como método para que os pontos
    /// de emissão não possam aplicar um e esquecer o outro, que era exatamente
    /// o risco de trocar um `apply_opacity` por duas chamadas soltas.
    pub fn aplicar_com_opacidade(&self, cor: u32, opacidade: f32) -> u32 {
        let c = self.aplicar(cor);
        if opacidade >= 1.0 {
            return c;
        }
        let a = (c & 0xFF) as f32 * opacidade.clamp(0.0, 1.0);
        (c & 0xFFFF_FF00) | (a.round().clamp(0.0, 255.0) as u32)
    }
}

/// Coeficientes de luminância do sRGB (Rec. 709) — os que o spec do
/// `grayscale`/`saturate` nomeia.
const LUM: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// Interpola entre a identidade e uma matriz-alvo, que é como o spec define
/// `grayscale(a)`, `sepia(a)` e `invert(a)`: o argumento é O QUANTO se aplica.
fn lerp_identidade(alvo: [[f32; 3]; 3], a: f32) -> [[f32; 3]; 3] {
    let mut m = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let id = if i == j { 1.0 } else { 0.0 };
            m[i][j] = id + (alvo[i][j] - id) * a;
        }
    }
    m
}

/// Converte UMA função de `filter` na sua matriz. `None` = não exprimível aqui,
/// o que recusa a cadeia inteira (ver o módulo).
fn funcao_para_matriz(nome: &str, arg: &str) -> Option<FilterMatriz> {
    // `blur` e `drop-shadow` caem no `_` no fim, e é lá que a recusa acontece —
    // deliberadamente sem braço próprio, para que uma função que ainda ninguém
    // viu (ou um `url(#svg)`) seja recusada pela mesma porta em vez de passar.
    let ident = FilterMatriz::IDENTIDADE;
    match nome {
        "brightness" => {
            let a = numero(arg)?;
            Some(FilterMatriz {
                m: lerp_identidade([[0.0; 3]; 3], 1.0 - a),
                ..ident
            })
        }
        "contrast" => {
            let a = numero(arg)?;
            // c*a + (0.5 - 0.5a): o deslocamento é o que mantém o meio-cinzento fixo.
            Some(FilterMatriz {
                m: [[a, 0.0, 0.0], [0.0, a, 0.0], [0.0, 0.0, a]],
                o: [0.5 - 0.5 * a; 3],
                a: 1.0,
            })
        }
        "invert" => {
            let a = numero(arg)?;
            // lerp(c, 1-c, a) = c*(1-2a) + a
            let k = 1.0 - 2.0 * a;
            Some(FilterMatriz {
                m: [[k, 0.0, 0.0], [0.0, k, 0.0], [0.0, 0.0, k]],
                o: [a; 3],
                a: 1.0,
            })
        }
        "grayscale" => {
            let a = numero(arg)?;
            Some(FilterMatriz {
                m: lerp_identidade([LUM, LUM, LUM], a),
                ..ident
            })
        }
        "sepia" => {
            let a = numero(arg)?;
            let sepia = [
                [0.393, 0.769, 0.189],
                [0.349, 0.686, 0.168],
                [0.272, 0.534, 0.131],
            ];
            Some(FilterMatriz {
                m: lerp_identidade(sepia, a),
                ..ident
            })
        }
        "saturate" => {
            let s = numero(arg)?;
            // Ao contrário do `grayscale`, o argumento NÃO é um quanto-se-aplica:
            // `saturate(2)` passa da identidade, e por isso a fórmula é a do spec
            // em vez de um lerp (que saturaria no máximo até à identidade).
            let mut m = [[0.0f32; 3]; 3];
            for i in 0..3 {
                for j in 0..3 {
                    let id = if i == j { 1.0 } else { 0.0 };
                    m[i][j] = LUM[j] + s * (id - LUM[j]);
                }
            }
            Some(FilterMatriz { m, ..ident })
        }
        "hue-rotate" => {
            let g = graus(arg)?;
            let (s, c) = g.to_radians().sin_cos();
            // A matriz do spec (Filter Effects 1 §8.6), literal.
            let m = [
                [
                    0.213 + c * 0.787 - s * 0.213,
                    0.715 - c * 0.715 - s * 0.715,
                    0.072 - c * 0.072 + s * 0.928,
                ],
                [
                    0.213 - c * 0.213 + s * 0.143,
                    0.715 + c * 0.285 + s * 0.140,
                    0.072 - c * 0.072 - s * 0.283,
                ],
                [
                    0.213 - c * 0.213 - s * 0.787,
                    0.715 - c * 0.715 + s * 0.715,
                    0.072 + c * 0.928 + s * 0.072,
                ],
            ];
            Some(FilterMatriz { m, ..ident })
        }
        "opacity" => {
            let a = numero(arg)?;
            Some(FilterMatriz {
                a: a.clamp(0.0, 1.0),
                ..ident
            })
        }
        _ => None,
    }
}

/// `1`, `0.5` ou `50%` → a fração. Recusa o que não for número (um `var()` por
/// resolver, por exemplo), porque um argumento que não se lê torna a função
/// inexprimível tal como um `blur` — e a cadeia cai pela mesma regra.
fn numero(arg: &str) -> Option<f32> {
    let t = arg.trim();
    if t.is_empty() {
        // `brightness()` sem argumento é inválido; `grayscale()` também. O spec
        // dá default só a algumas e o valor difere entre elas — recusar é mais
        // honesto do que inventar um.
        return None;
    }
    if let Some(p) = t.strip_suffix('%') {
        return p
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v / 100.0)
            .filter(|v| *v >= 0.0);
    }
    t.parse::<f32>().ok().filter(|v| *v >= 0.0)
}

/// `90deg`, `1.5rad`, `0.25turn`, `100grad` → graus. Um número nu conta como
/// graus (`hue-rotate(90)`), que é o que os browsers aceitam.
fn graus(arg: &str) -> Option<f32> {
    let t = arg.trim();
    for (suf, fator) in [
        ("deg", 1.0),
        ("grad", 0.9),
        ("turn", 360.0),
        ("rad", 180.0 / std::f32::consts::PI),
    ] {
        if let Some(n) = t.strip_suffix(suf) {
            return n.trim().parse::<f32>().ok().map(|v| v * fator);
        }
    }
    t.parse::<f32>().ok()
}

/// Lê o valor de `filter` inteiro.
///
/// Devolve SEMPRE uma matriz — a identidade quando a cadeia é `none`, vazia, ou
/// contém alguma função que não sabemos fazer. Devolver `Option` e deixar o
/// produtor decidir seria a mesma decisão escrita em dois sítios, e o sítio
/// errado para a tomar: é aqui que se sabe PORQUÊ.
pub fn filtro(valor: &str) -> FilterMatriz {
    let v = valor.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("none") {
        return FilterMatriz::IDENTIDADE;
    }
    let mut acc = FilterMatriz::IDENTIDADE;
    for (nome, arg) in match funcoes(v) {
        Some(fs) => fs,
        // Parêntese por fechar, ou texto solto entre funções: valor inválido.
        None => return FilterMatriz::IDENTIDADE,
    } {
        match funcao_para_matriz(&nome.to_ascii_lowercase(), &arg) {
            Some(f) => acc = acc.compor(&f),
            // TUDO-OU-NADA: uma função inexprimível deixa o elemento como
            // estava. Ver o cabeçalho do módulo — aplicar o resto da cadeia
            // seria um terceiro desenho que ninguém pediu.
            None => return FilterMatriz::IDENTIDADE,
        }
    }
    acc
}

/// Parte `a(1) b(2 3)` em `[("a","1"), ("b","2 3")]`. `None` se a string não é
/// uma sequência de chamadas bem formada.
fn funcoes(v: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut resto = v;
    loop {
        let resto_t = resto.trim_start();
        if resto_t.is_empty() {
            return Some(out);
        }
        let abre = resto_t.find('(')?;
        let nome = resto_t[..abre].trim();
        if nome.is_empty() || !nome.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        // Conta parênteses: o argumento pode ter os seus (`drop-shadow` com um
        // `rgba(...)` dentro). Sem isto, a primeira `)` fechava a função errada
        // e o que sobrava era lixo — e lixo devolve identidade, portanto o erro
        // ficaria invisível em vez de recusar.
        let mut nivel = 0i32;
        let mut fim = None;
        for (i, ch) in resto_t[abre..].char_indices() {
            match ch {
                '(' => nivel += 1,
                ')' => {
                    nivel -= 1;
                    if nivel == 0 {
                        fim = Some(abre + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let fim = fim?;
        out.push((nome.to_owned(), resto_t[abre + 1..fim].to_owned()));
        resto = &resto_t[fim + 1..];
    }
}

/// Lê `clip-path` e devolve o retângulo a que recortar, em coordenadas do
/// MESMO espaço de `caixa` (o border-box do elemento).
///
/// `None` quer dizer "não recorta", e cobre três casos que valem a mesma coisa
/// para quem pinta: `none`, uma forma que não sabemos fazer, e um valor
/// inválido. Só `inset()` sem `round` chega a devolver `Some` — ver o cabeçalho
/// do módulo para o porquê de `polygon()` não ser aproximado pela envolvente.
pub fn clip_retangulo(valor: &str, caixa: Rect) -> Option<Rect> {
    let v = valor.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("none") {
        return None;
    }
    let fs = funcoes(v)?;
    // Uma forma só. `clip-path: inset(...) border-box` traz uma caixa de
    // referência que não é função e cai fora deste parse — e recusar é correto:
    // `border-box` é o que já assumimos, mas `content-box` mudaria o resultado e
    // não temos aqui a caixa para o fazer.
    let [(nome, arg)] = &fs[..] else { return None };
    if !nome.eq_ignore_ascii_case("inset") {
        return None;
    }
    let arg = arg.trim();
    // `round` faz cantos arredondados, que um recorte retangular não tem como
    // dar. Recortar reto o que devia ser redondo é visível, portanto recusa.
    if arg.to_ascii_lowercase().contains("round") {
        return None;
    }
    let partes: Vec<&str> = arg.split_whitespace().collect();
    // 1 a 4 valores, na ordem do CSS: cima, direita, baixo, esquerda.
    let (t, r, b, l) = match partes.len() {
        1 => (partes[0], partes[0], partes[0], partes[0]),
        2 => (partes[0], partes[1], partes[0], partes[1]),
        3 => (partes[0], partes[1], partes[2], partes[1]),
        4 => (partes[0], partes[1], partes[2], partes[3]),
        _ => return None,
    };
    // As percentagens de cima/baixo são da ALTURA e as dos lados da LARGURA.
    let t = comprimento(t, caixa.h)?;
    let b = comprimento(b, caixa.h)?;
    let l = comprimento(l, caixa.w)?;
    let r = comprimento(r, caixa.w)?;
    let x = caixa.x + l;
    let y = caixa.y + t;
    // Lados que se cruzam dão área zero, e não área negativa: é o que o spec diz
    // e é também o que o backend precisa (um rect de largura negativa desenharia
    // ao contrário em vez de não desenhar).
    let w = (caixa.w - l - r).max(0.0);
    let h = (caixa.h - t - b).max(0.0);
    Some(Rect::new(x, y, w, h))
}

/// `12px`, `10%` ou `0` → px, dado o tamanho de referência do eixo. Recusa
/// unidades relativas à fonte: resolver um `em` exige o estilo do elemento, que
/// esta função não tem — e adivinhar 16 daria um recorte errado num elemento
/// com outra fonte.
fn comprimento(v: &str, referencia: f32) -> Option<f32> {
    let t = v.trim();
    if let Some(p) = t.strip_suffix('%') {
        return p.trim().parse::<f32>().ok().map(|n| n / 100.0 * referencia);
    }
    if let Some(n) = t.strip_suffix("px") {
        return n.trim().parse::<f32>().ok();
    }
    t.parse::<f32>().ok().filter(|n| *n == 0.0)
}

#[cfg(test)]
mod tests;
