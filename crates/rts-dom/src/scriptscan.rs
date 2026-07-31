//! Varredura léxica do JS de página, em RUST.
//!
//! O pré-passo que adapta um `<script>` ao subset do motor (`scriptscope.ts`)
//! precisa varrer o fonte caractere a caractere várias vezes: achar os trechos
//! que são CÓDIGO (fora de string/comentário/template), os corpos de classe, os
//! globais implícitos (`x = …` sem declaração) e os nomes declarados.
//!
//! Escrito em `.ts`, isso custava ~1 µs por caractere — num bundle real da Meta
//! (6 MB) o pré-passo levava **49 s** contra **113 ms** de compilação de
//! verdade. Não era loop infinito nem bundle patológico: era varrer 6 milhões de
//! caracteres com `substring(i, i+1)` num laço `.ts`. Em Rust é uma passada
//! sobre `&[u8]`.
//!
//! O `.ts` continua dono da REESCRITA (montar o texto final com `__G.`); o que
//! mudou de lado é só a varredura, que é o que escala com o tamanho do arquivo.

use rts_engine::abi::ty::Handle;

fn is_id_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b >= 0x80
}

fn is_id_part(b: u8) -> bool {
    is_id_start(b) || b.is_ascii_digit()
}

/// Um `/` inicia REGEX (e não divisão) quando o último token significativo não
/// pode terminar uma expressão. Mesma heurística do `.ts`.
fn slash_starts_regex(last: &[u8]) -> bool {
    match last {
        b")" | b"]" | b"}" | b"" => false,
        t if t.len() == 1 && (t[0].is_ascii_alphanumeric() || t[0] == b'_' || t[0] == b'$') => false,
        // palavra-chave antes de `/` ⇒ regex (`return /re/`, `typeof /re/`)
        t if t.iter().all(|c| c.is_ascii_alphabetic()) => matches!(
            t,
            b"return" | b"typeof" | b"case" | b"in" | b"of" | b"delete" | b"void"
                | b"instanceof" | b"new" | b"do" | b"else" | b"yield" | b"await"
        ),
        _ => true,
    }
}

/// Os intervalos `[ini, fim)` que são CÓDIGO — fora de string, template,
/// comentário e literal de regex. É a base de todo pass: sem isto, um `var` ou
/// um `instanceof` DENTRO de uma string seria tratado como programa.
fn code_spans(src: &[u8]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let n = src.len();
    let mut i = 0usize;
    let mut run_start = 0usize;
    let mut last_tok: Vec<u8> = Vec::new();
    // Profundidade de `{` por template aberto; vazio = fora de template.
    let mut tpl_brace: Vec<i32> = Vec::new();

    while i < n {
        let c = src[i];

        // `}` fechando um `${…}`: volta ao TEXTO do template.
        if c == b'}' && !tpl_brace.is_empty() {
            let top = *tpl_brace.last().unwrap();
            if top == 0 {
                tpl_brace.pop();
                spans.push((run_start, i));
                i += 1;
                let mut closed = false;
                while i < n {
                    let d = src[i];
                    if d == b'\\' {
                        i += 2;
                        continue;
                    }
                    if d == b'`' {
                        i += 1;
                        closed = true;
                        break;
                    }
                    if d == b'$' && i + 1 < n && src[i + 1] == b'{' {
                        tpl_brace.push(0);
                        i += 2;
                        run_start = i;
                        closed = true;
                        break;
                    }
                    i += 1;
                }
                if !closed {
                    run_start = i;
                    break;
                }
                if tpl_brace.is_empty() {
                    run_start = i;
                    last_tok = b"`".to_vec();
                }
                continue;
            }
            *tpl_brace.last_mut().unwrap() = top - 1;
            last_tok = b"}".to_vec();
            i += 1;
            continue;
        }
        if c == b'{' && !tpl_brace.is_empty() {
            *tpl_brace.last_mut().unwrap() += 1;
            last_tok = b"{".to_vec();
            i += 1;
            continue;
        }

        if c == b'/' && i + 1 < n {
            let c2 = src[i + 1];
            if c2 == b'/' {
                spans.push((run_start, i));
                while i < n && src[i] != b'\n' {
                    i += 1;
                }
                run_start = i;
                continue;
            }
            if c2 == b'*' {
                spans.push((run_start, i));
                i += 2;
                while i + 1 < n && !(src[i] == b'*' && src[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(n);
                run_start = i;
                continue;
            }
            if slash_starts_regex(&last_tok) {
                spans.push((run_start, i));
                i += 1;
                let mut in_class = false;
                while i < n {
                    let d = src[i];
                    if d == b'\\' {
                        i += 2;
                        continue;
                    }
                    if d == b'[' {
                        in_class = true;
                    } else if d == b']' {
                        in_class = false;
                    } else if d == b'/' && !in_class {
                        i += 1;
                        break;
                    } else if d == b'\n' {
                        break;
                    }
                    i += 1;
                }
                // flags
                while i < n && is_id_part(src[i]) {
                    i += 1;
                }
                run_start = i;
                last_tok = b"/".to_vec();
                continue;
            }
        }

        if c == b'"' || c == b'\'' {
            spans.push((run_start, i));
            let q = c;
            i += 1;
            while i < n {
                let d = src[i];
                if d == b'\\' {
                    i += 2;
                    continue;
                }
                if d == q {
                    i += 1;
                    break;
                }
                i += 1;
            }
            run_start = i;
            last_tok = b"\"".to_vec();
            continue;
        }

        if c == b'`' {
            spans.push((run_start, i));
            i += 1;
            let mut closed = false;
            while i < n {
                let d = src[i];
                if d == b'\\' {
                    i += 2;
                    continue;
                }
                if d == b'`' {
                    i += 1;
                    closed = true;
                    break;
                }
                if d == b'$' && i + 1 < n && src[i + 1] == b'{' {
                    tpl_brace.push(0);
                    i += 2;
                    run_start = i;
                    closed = true;
                    break;
                }
                i += 1;
            }
            if !closed {
                run_start = i;
                break;
            }
            if tpl_brace.is_empty() {
                run_start = i;
                last_tok = b"`".to_vec();
            }
            continue;
        }

        if is_id_start(c) {
            let s = i;
            while i < n && is_id_part(src[i]) {
                i += 1;
            }
            last_tok = src[s..i].to_vec();
            continue;
        }
        if !c.is_ascii_whitespace() {
            last_tok = vec![c];
        }
        i += 1;
    }
    if run_start < n {
        spans.push((run_start, n));
    }
    spans
}

/// Intervalos `[ini, fim]` de CORPO DE CLASSE, ordenados por abertura.
///
/// `class C { nome = 1 }` declara um CAMPO, não um global implícito — sem isto
/// o scanner reportava `nome` e a reescrita produzia `class C { __G.nome = 1 }`,
/// um SyntaxError no código que este próprio módulo gerou.
fn class_body_ranges(src: &[u8], spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut open_is_class: Vec<bool> = Vec::new();
    let mut open_at: Vec<usize> = Vec::new();
    let mut saw_class_head = false;

    for &(start, end) in spans {
        let mut i = start;
        while i < end {
            let c = src[i];
            if is_id_start(c) {
                let s = i;
                while i < end && is_id_part(src[i]) {
                    i += 1;
                }
                if &src[s..i] == b"class" {
                    saw_class_head = true;
                }
                continue;
            }
            if c == b'{' {
                open_is_class.push(saw_class_head);
                open_at.push(i);
                saw_class_head = false;
            } else if c == b'}' {
                if let (Some(was_class), Some(at)) = (open_is_class.pop(), open_at.pop()) {
                    if was_class {
                        ranges.push((at, i));
                    }
                }
            } else if c == b';' {
                saw_class_head = false;
            }
            i += 1;
        }
    }
    // Saem em ordem de FECHAMENTO (aninhada fecha antes da externa); a consulta
    // por posição quer ordem de abertura.
    ranges.sort_unstable_by_key(|r| r.0);
    ranges
}

fn in_ranges(ranges: &[(usize, usize)], pos: usize) -> bool {
    ranges
        .binary_search_by(|r| {
            if pos <= r.0 {
                std::cmp::Ordering::Greater
            } else if pos >= r.1 {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Nomes DECLARADOS pelo script (`var`/`let`/`const`/`function`/`class`/`catch`
/// e parâmetros de função). Um nome daqui nunca é global implícito.
///
/// A varredura é CONTÍNUA sobre o texto: os spans dizem apenas o que IGNORAR
/// (conteúdo de string/comentário/regex), nunca onde PARAR. A versão anterior
/// iterava por span e terminava a lista de declaradores na fronteira — e uma
/// cadeia `var a={…},b="txt",c=function(){}` atravessa spans a cada literal,
/// então tudo depois do primeiro literal ficava fora de `declared`. Consequência
/// real: no bundle da Meta, `Me` (declarado no meio de uma cadeia de 5 KB) era
/// classificado como global implícito, virava `__G.Me` numa posição de BINDING
/// (`var …,__G.Me=function(){}`) e o parser recusava — `var a.b = …` não é JS.
fn scan_declared(src: &[u8], spans: &[(usize, usize)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let n = src.len();
    // `code[i]` = o byte `i` está em trecho de CÓDIGO (fora de string/comentário).
    let mut code = vec![false; n];
    for &(s, e) in spans {
        for c in code.iter_mut().take(e.min(n)).skip(s.min(n)) {
            *c = true;
        }
    }
    let mut i = 0usize;
    while i < n {
        if !code[i] || !is_id_start(src[i]) {
            i += 1;
            continue;
        }
        let s = i;
        while i < n && code[i] && is_id_part(src[i]) {
            i += 1;
        }
        let w = &src[s..i];
        let is_decl = w == b"var" || w == b"let" || w == b"const";
        let is_fn = w == b"function" || w == b"class" || w == b"catch";
        if !(is_decl || is_fn) {
            continue;
        }
        // `var a = 1, b = 2` → a lista inteira até `;`/`)`, atravessando os
        // literais que aparecerem no meio (só o CONTEÚDO deles é ignorado).
        let mut j = i;
        let mut guard = 0;
        while j < n && guard < 4096 {
            guard += 1;
            while j < n && (!code[j] || src[j].is_ascii_whitespace()) {
                j += 1;
            }
            if j < n && is_id_start(src[j]) {
                let ns = j;
                while j < n && code[j] && is_id_part(src[j]) {
                    j += 1;
                }
                let name = String::from_utf8_lossy(&src[ns..j]).into_owned();
                if !out.contains(&name) {
                    out.push(name);
                }
            } else if j < n && (src[j] == b'(' || src[j] == b'{' || src[j] == b'[') {
                // params / destructuring: entra e segue colhendo nomes
                j += 1;
                continue;
            }
            while j < n && (!code[j] || src[j].is_ascii_whitespace()) {
                j += 1;
            }
            if j >= n {
                break;
            }
            // continua a lista só em `,`; qualquer outra coisa encerra
            if src[j] == b',' {
                j += 1;
                continue;
            }
            if src[j] == b'=' {
                // pula o inicializador até a vírgula de topo ou o fim
                let mut depth = 0i32;
                while j < n {
                    if !code[j] {
                        j += 1;
                        continue;
                    }
                    let d = src[j];
                    if d == b'(' || d == b'[' || d == b'{' {
                        depth += 1;
                    } else if d == b')' || d == b']' || d == b'}' {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    } else if (d == b',' || d == b';') && depth == 0 {
                        break;
                    }
                    j += 1;
                }
                if j < n && src[j] == b',' {
                    j += 1;
                    continue;
                }
            }
            break;
        }
        i = i.max(s + 1);
    }
    out
}

/// Os nomes que o script cria SEM declarar (`requireLazy = function(){…}`) — os
/// globais implícitos que precisam ir para o saco compartilhado.
fn scan_implicit(src: &[u8]) -> Vec<String> {
    let spans = code_spans(src);
    let class_ranges = class_body_ranges(src, &spans);
    let declared = scan_declared(src, &spans);
    let mut out: Vec<String> = Vec::new();

    for &(start, end) in &spans {
        let mut i = start;
        while i < end {
            if !is_id_start(src[i]) {
                i += 1;
                continue;
            }
            let s = i;
            while i < end && is_id_part(src[i]) {
                i += 1;
            }
            let word = &src[s..i];
            // `.nome` é campo, não global
            let mut p = s as isize - 1;
            while p >= 0 && src[p as usize].is_ascii_whitespace() {
                p -= 1;
            }
            let prev = if p >= 0 { src[p as usize] } else { 0 };
            if prev == b'.' {
                continue;
            }
            // procura o `=` adiante
            let mut j = i;
            while j < end && src[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= end || src[j] != b'=' {
                continue;
            }
            let nx = if j + 1 < end { src[j + 1] } else { 0 };
            // `==`/`===`/`=>` não são atribuição
            if nx == b'=' || nx == b'>' || prev == b'=' || prev == b'!' || prev == b'<' || prev == b'>'
            {
                continue;
            }
            // palavra-chave declarante imediatamente antes?
            let we = (p + 1).max(0) as usize;
            let mut ws = we;
            while ws > 0 && is_id_part(src[ws - 1]) {
                ws -= 1;
            }
            let kw = &src[ws..we];
            if matches!(
                kw,
                b"var" | b"let" | b"const" | b"function" | b"class" | b"case" | b"return"
            ) {
                continue;
            }
            if in_ranges(&class_ranges, s) {
                continue;
            }
            let name = String::from_utf8_lossy(word).into_owned();
            if !declared.contains(&name) && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// `function(){ … arguments … }` → `function(...__rtsargs){ … __rtsargs … }`.
///
/// O motor não tem o objeto `arguments`, mas suporta rest; para o uso dominante
/// em loaders (repassar os args adiante) as formas são equivalentes. Só age em
/// lista de parâmetros VAZIA cujo corpo menciona `arguments` — com params
/// nomeados a equivalência não vale e o texto fica como está.
fn rewrite_arguments(src: &[u8]) -> Vec<u8> {
    if find_sub(src, b"arguments").is_none() {
        return src.to_vec();
    }
    let n = src.len();
    let mut out: Vec<u8> = Vec::with_capacity(n + 64);
    let mut run_start = 0usize;
    let mut i = 0usize;
    while i < n {
        if src[i..].starts_with(b"function") {
            let mut j = i + 8;
            while j < n && src[j].is_ascii_whitespace() {
                j += 1;
            }
            while j < n && is_id_part(src[j]) {
                j += 1;
            }
            while j < n && src[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < n && src[j] == b'(' {
                let mut k = j + 1;
                while k < n && src[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < n && src[k] == b')' {
                    let mut b = k + 1;
                    while b < n && src[b] != b'{' {
                        b += 1;
                    }
                    let mut depth = 0i32;
                    let mut e = b;
                    while e < n {
                        if src[e] == b'{' {
                            depth += 1;
                        } else if src[e] == b'}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        e += 1;
                    }
                    let body_end = (e + 1).min(n);
                    let body = &src[b..body_end];
                    if find_sub(body, b"arguments").is_some() {
                        out.extend_from_slice(&src[run_start..j]);
                        out.extend_from_slice(b"(...__rtsargs)");
                        out.extend_from_slice(&replace_ident(body, b"arguments", b"__rtsargs"));
                        i = body_end;
                        run_start = i;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out.extend_from_slice(&src[run_start..n]);
    out
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Troca ocorrências do identificador `from` por `to` (identificador inteiro, e
/// não `.from` nem dentro de outro nome).
fn replace_ident(src: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(src.len());
    let mut run_start = 0usize;
    let n = src.len();
    let mut i = 0usize;
    while i < n {
        if is_id_start(src[i]) {
            let s = i;
            while i < n && is_id_part(src[i]) {
                i += 1;
            }
            if &src[s..i] == from {
                let mut p = s as isize - 1;
                while p >= 0 && src[p as usize].is_ascii_whitespace() {
                    p -= 1;
                }
                let prev = if p >= 0 { src[p as usize] } else { 0 };
                if prev != b'.' {
                    out.extend_from_slice(&src[run_start..s]);
                    out.extend_from_slice(to);
                    run_start = i;
                }
            }
            continue;
        }
        i += 1;
    }
    out.extend_from_slice(&src[run_start..n]);
    out
}

/// `a=1, b=function(){}` → `a=1; b=function(){}`.
///
/// O motor aceita o operador vírgula e aceita `function` como valor, mas não a
/// COMBINAÇÃO numa só expressão (a extração da função aborta) — e minificador
/// gera isso o tempo todo. Só quebra vírgulas em profundidade ZERO de `()`,
/// `[]`, `{}` e fora de string/comentário, então vírgula de argumento, de array
/// e de objeto fica intacta.
fn split_top_level_sequences(src: &[u8]) -> Vec<u8> {
    let n = src.len();
    let mut out: Vec<u8> = Vec::with_capacity(n + 32);
    let mut run_start = 0usize;
    let mut i = 0usize;
    let (mut par, mut brk, mut brc) = (0i32, 0i32, 0i32);
    while i < n {
        let c = src[i];
        if c == b'/' && i + 1 < n {
            let c2 = src[i + 1];
            if c2 == b'/' {
                while i < n && src[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if c2 == b'*' {
                i += 2;
                while i + 1 < n && !(src[i] == b'*' && src[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(n);
                continue;
            }
        }
        if c == b'"' || c == b'\'' || c == b'`' {
            let q = c;
            i += 1;
            while i < n {
                let d = src[i];
                if d == b'\\' {
                    i += 2;
                    continue;
                }
                if d == q {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        match c {
            b'(' => par += 1,
            b')' => par -= 1,
            b'[' => brk += 1,
            b']' => brk -= 1,
            b'{' => brc += 1,
            b'}' => brc -= 1,
            _ => {}
        }
        if c == b',' && par == 0 && brk == 0 && brc == 0 {
            out.extend_from_slice(&src[run_start..i]);
            out.push(b';');
            run_start = i + 1;
        }
        i += 1;
    }
    out.extend_from_slice(&src[run_start..n]);
    out
}

/// `x instanceof window.C` → `x instanceof C`.
///
/// O motor resolve `instanceof` por nome de classe registrada; qualificar com
/// `window.` fazia a comparação virar ReferenceError. Só age imediatamente
/// depois do token `instanceof`, e só dentro de CÓDIGO (uma string que contenha
/// o texto "instanceof" fica intacta — era um bug real do pass `.ts`).
fn unqualify_instanceof(src: &[u8]) -> Vec<u8> {
    if find_sub(src, b"instanceof").is_none() {
        return src.to_vec();
    }
    let spans = code_spans(src);
    let n = src.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut run_start = 0usize;
    for &(start, end) in &spans {
        let mut i = start;
        while i < end {
            if i + 10 <= end && &src[i..i + 10] == b"instanceof" {
                let antes = if i > 0 { src[i - 1] } else { 0 };
                let depois = if i + 10 < n { src[i + 10] } else { 0 };
                if !is_id_part(antes) && !is_id_part(depois) {
                    let mut j = i + 10;
                    while j < end && src[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    let base_start = j;
                    while j < end && is_id_part(src[j]) {
                        j += 1;
                    }
                    let base = &src[base_start..j];
                    let eh_global = matches!(
                        base,
                        b"window" | b"globalThis" | b"self" | b"top" | b"parent"
                    );
                    if eh_global && j < end && src[j] == b'.' {
                        out.extend_from_slice(&src[run_start..i + 10]);
                        out.push(b' ');
                        i = j + 1;
                        run_start = i;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }
    out.extend_from_slice(&src[run_start..n]);
    out
}

// O resultado da última varredura, para o `.ts` ler item a item (um `Vec<String>`
// não atravessa a borda; índice + `nameAt` atravessa).
thread_local! {
    static LAST: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Scanner léxico do JS de página. Só membros estáticos.
///
/// Classe (e não `#[rtse::function]` livre) porque o `rts-symbol-baker` ainda
/// não escaneia funções livres — o símbolo não entraria na tabela.
#[rtse::class("ScriptScan")]
#[derive(Clone, Default)]
pub struct ScriptScan {}

#[rtse::class("ScriptScan")]
impl ScriptScan {
    /// Varre `src` e guarda os GLOBAIS IMPLÍCITOS; devolve quantos achou.
    /// Leia os nomes com `nameAt(i)`.
    #[rtse::statical]
    fn implicitGlobals(src: &str) -> f64 {
        let found = scan_implicit(src.as_bytes());
        let n = found.len();
        LAST.with(|l| *l.borrow_mut() = found);
        n as f64
    }

    /// Varre `src` e guarda os nomes DECLARADOS; devolve quantos achou.
    #[rtse::statical]
    fn declaredNames(src: &str) -> f64 {
        let bytes = src.as_bytes();
        let spans = code_spans(bytes);
        let found = scan_declared(bytes, &spans);
        let n = found.len();
        LAST.with(|l| *l.borrow_mut() = found);
        n as f64
    }

    /// O n-ésimo nome da última varredura ("" fora de faixa).
    #[rtse::statical]
    fn nameAt(n: f64) -> String {
        let idx = n as usize;
        LAST.with(|l| l.borrow().get(idx).cloned().unwrap_or_default())
    }

    /// Diagnóstico: quantos trechos de CÓDIGO (fora de string/comentário).
    #[rtse::statical]
    fn codeSpanCount(src: &str) -> f64 {
        code_spans(src.as_bytes()).len() as f64
    }

    /// TODA a normalização sintática numa passada: `arguments` → rest, sequência
    /// de topo → statements, `instanceof window.C` → `instanceof C`. É o texto
    /// que vai ser compilado (e sobre o qual os nomes são detectados) — os dois
    /// lados TÊM de ver exatamente a mesma string.
    #[rtse::statical]
    fn normalize(src: &str) -> String {
        let bytes = src.as_bytes();
        let a = rewrite_arguments(bytes);
        let b = split_top_level_sequences(&a);
        let c = unqualify_instanceof(&b);
        String::from_utf8_lossy(&c).into_owned()
    }

    /// Placeholder de handle para a assinatura de classe (não usado).
    #[rtse::statical]
    fn noop(_h: Handle) -> f64 {
        0.0
    }
}
