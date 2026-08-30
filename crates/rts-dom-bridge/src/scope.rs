//! O saco de globais de cada documento — o objeto global que todos os
//! `<script>` de uma página compartilham.
//!
//! # Por que isto existe
//!
//! Cada `<script>` é compilado por `new Function`, ou seja, um PROGRAMA novo.
//! Um `var` de um programa não é alcançável do seguinte, então uma página cujo
//! primeiro script define `__d` e cujo segundo o chama não funcionaria: o saco
//! é o que dá aos dois a mesma superfície. O `.ts` expõe-o como um Proxy cujas
//! traps `get`/`set` caem aqui.
//!
//! # O que este módulo aprendeu do que veio antes
//!
//! Havia um `rts-dom/src/scriptscope.rs` que fazia isto para o motor antigo, e
//! foi apagado com ele em `46910d997` — nomeava `rts_engine::abi::ty::Poly` e
//! `rts_engine::heap::handles::mark_handle`, dois nomes de um crate que deixou
//! de existir. O Rust compilou depois disso; o prelude `.ts` ficou a chamar um
//! `DomScope` que já não estava lá, e nada em Rust compila o `.ts` — por isso
//! seis fixtures de DOM morriam em `ReferenceError` sem que a suite Rust, verde,
//! tivesse como notar. Duas das suas lições sobrevivem e uma não:
//!
//! **Continua a valer: GLOBAL, e não `thread_local`.** A trap `set` do Proxy
//! roda num contexto de execução próprio; com um mapa por thread a escrita caía
//! num sítio que o leitor de fora nunca via, e `count` respondia 0 logo a seguir
//! a um `set` bem-sucedido. Foi assim que o `__d` da Meta (o registador de
//! módulos) desapareceu entre o script que o define e os bundles que o chamam.
//!
//! **Deixou de ser preciso: guardar o valor como word tagueada.** Era metade
//! daquele ficheiro — com `f64` o round-trip devolvia `undefined` e uma função
//! voltava não-chamável. Aqui um nativo já recebe e devolve `u64` opaco, então
//! não há coerção nenhuma a evitar.
//!
//! # Onde os valores vivem, e por que não num mapa daqui
//!
//! Os valores ficam num OBJETO DO RUNTIME por documento, não num `HashMap`
//! deste crate. A diferença é o coletor: um objeto é traçado pelas suas
//! propriedades como qualquer outro, enquanto um mapa em memória do Rust é
//! invisível ao mark — que é exatamente o use-after-free do #2069, onde um ciclo
//! de GC no meio da execução libertava um `__d` ainda vivo e o próximo `__G.x`
//! dereferenciava um slot já libertado.
//!
//! O que mantém o objeto vivo é [`rts_core::entry::hold_current`], **um por
//! documento**. O mecanismo é o dos `napi_ref` e a sua documentação descreve
//! este caso em palavras: um nativo que guarda um valor depois de a chamada
//! retornar não é um frame, e nenhuma varredura de pilha o alcança.
//!
//! Um hold por DOCUMENTO e não um por global, e isso é deliberado: a doc do
//! `external` diz que quem guarda um destes guarda um punhado, e `release`
//! varre a lista. O saco de globais de um bundle grande são centenas de nomes
//! com re-atribuições, o que faria de cada `set` duas varreduras lineares.
//! Ao guardar só o objeto, cada `set` passa a ser uma escrita de propriedade
//! normal — com shape e inline cache — e o punhado volta a ser um punhado.
//!
//! # A ordem fica em Rust
//!
//! `count`/`nameAt` enumeram os nomes publicados, e um objeto do runtime não
//! oferece enumeração a este crate. Então a ORDEM é um `Vec<String>` aqui —
//! texto, sem nada que o coletor precise de ver — e os VALORES ficam no objeto.
//! É a única coisa que este módulo guarda por si.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use rts_core::entry::{self, Provided};

use crate::value::{handle, int, integer, nothing, string, text};

/// O saco de um documento.
struct Bag {
    /// O objeto do runtime onde os valores estão, como propriedades.
    object: u64,
    /// O identificador que o mantém vivo. Solto em [`drop_scope`].
    hold: u32,
    /// O que o último `<script>` que falhou disse, até alguém o ler.
    last_error: Option<String>,
    /// Os nomes por ordem de publicação, para `count`/`nameAt`.
    order: Vec<String>,
    /// O mesmo conjunto, para decidir em O(1) se um `set` publica um nome novo.
    /// Sem ele, cada escrita varreria `order` — e uma página que reatribui um
    /// contador num laço escreve muito mais vezes do que publica.
    known: HashSet<String>,
}

/// GLOBAL, e não `thread_local` — ver a nota no topo do módulo. É a única razão
/// pela qual esta função existe em vez de um `thread_local!`.
fn bags() -> &'static Mutex<HashMap<u64, Bag>> {
    static BAGS: OnceLock<Mutex<HashMap<u64, Bag>>> = OnceLock::new();
    BAGS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Os sacos, com o lock recuperado se alguma thread entrou em pânico a segurá-lo.
///
/// Um pânico aqui não corrompe o mapa — as operações são inserções e leituras —
/// então envenenar o saco de globais de uma página inteira seria transformar um
/// erro isolado numa página morta.
fn locked() -> std::sync::MutexGuard<'static, HashMap<u64, Bag>> {
    match bags().lock() {
        Ok(bags) => bags,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// O objeto do documento `h`, criando-o na primeira escrita.
///
/// O objeto é criado FORA do lock e só depois inserido, porque `object_new` e
/// `hold_current` entram no runtime — alocam, e alocar pode disparar uma
/// coleção que volta a entrar aqui. Segurar o lock durante isso é o deadlock
/// que o módulo anterior documentava ao soltar o seu antes de marcar.
///
/// A corrida que isso abre é resolvida na inserção: se outro caminho criou o
/// saco entretanto, o objeto acabado de criar é solto e vence o que já lá está,
/// para que os dois lados vejam UM saco e não dois.
fn object_for(h: u64) -> u64 {
    if let Some(object) = locked().get(&h).map(|bag| bag.object) {
        return object;
    }
    let object = entry::object_new(0);
    let hold = entry::hold_current(object);
    let mut bags = locked();
    if let Some(existing) = bags.get(&h).map(|bag| bag.object) {
        drop(bags);
        entry::release_current(hold);
        return existing;
    }
    bags.insert(
        h,
        Bag {
            object,
            last_error: None,
            hold,
            order: Vec::new(),
            known: HashSet::new(),
        },
    );
    object
}

/// Regista `name` como publicado, se ainda não estava.
fn remember(h: u64, name: &str) {
    let mut bags = locked();
    if let Some(bag) = bags.get_mut(&h)
        && bag.known.insert(name.to_string())
    {
        bag.order.push(name.to_string());
    }
}

/// Descarta o saco do documento `h`, e solta o que o mantinha vivo.
///
/// Chamado pelo `free` do documento. Sem o `release`, o objeto — e tudo o que a
/// página lhe pendurou — ficaria vivo até o processo terminar, que é uma fuga
/// proporcional ao número de documentos que um servidor renderiza.
pub fn drop_scope(h: u64) {
    let hold = locked().remove(&h).map(|bag| bag.hold);
    if let Some(hold) = hold {
        entry::release_current(hold);
    }
}

pub const MEMBERS: &[(&str, Provided)] = &[
    ("count", count),
    ("nameAt", name_at),
    ("get", get),
    ("set", set),
    ("has", has),
    ("drop", drop_member),
    ("run", run),
    ("lastError", last_error),
    ("adopt", adopt),
];

/// `DomScope.adopt(h, objeto)` — faz de `objeto` o saco de globais deste
/// documento.
///
/// # Porque o escopo tem de SER o `window`
///
/// Num browser o objeto global e o `window` são a mesma coisa: um script que
/// escreve `window.X = 1` e outro que lê `X` como nome livre encontram-se,
/// porque estão a falar da mesma propriedade do mesmo objeto.
///
/// Aqui eram dois. O `window` era publicado COMO PROPRIEDADE do saco, e medido
/// dava isto: `window.X = 42` no primeiro script, `typeof X` no segundo →
/// `"undefined"`; e ao contrário, `Z = 9` livre não aparecia em `window.Z`.
///
/// O que isso quebra não é um caso de canto — é o formato UMD, que é como
/// TODA a biblioteca do npm é servida a uma página. O ramo de browser de um
/// UMD faz `factory(global.React = {})`, e o script seguinte lê `React`. Com
/// dois objetos, o React 18.3.1 publicava-se num sítio que o programa nunca
/// via: `ReferenceError: React is not defined`, com os dois bundles a terem
/// corrido sem um erro.
///
/// Adotar em vez de copiar: uma cópia teria de ser mantida em dia nos dois
/// sentidos e toda a escrita passaria a ter dois destinos, que é a forma de
/// eles divergirem num deles.
/// Responde `1` quando este documento JÁ tinha adotado este objeto, para que
/// quem prepara o escopo possa sair sem repetir o trabalho — em vez de o
/// marcar com uma propriedade, que ficaria à vista do JavaScript da página.
extern "C" fn adopt(_e: u64, _t: u64, doc: u64, object: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    if locked().get(&h).map(|bag| bag.object) == Some(object) {
        return int(1);
    }
    // O `hold` novo ANTES de largar o antigo: entre os dois há uma alocação
    // possível, e uma coleção nesse intervalo não pode encontrar o documento
    // sem saco nenhum.
    let hold = entry::hold_current(object);
    let anterior = {
        let mut bags = locked();
        match bags.get_mut(&h) {
            Some(bag) => {
                let anterior = Some((bag.object, bag.hold));
                bag.object = object;
                bag.hold = hold;
                anterior
            }
            None => {
                bags.insert(
                    h,
                    Bag { object, hold, last_error: None, order: Vec::new(), known: HashSet::new() },
                );
                None
            }
        }
    };
    if let Some((_, hold)) = anterior {
        entry::release_current(hold);
    }
    int(0)
}

/// `DomScope.run(h, fonte, window)` — corre o texto de um `<script>` com o saco
/// deste documento COMO ESCOPO, e devolve `1` se correu.
///
/// # Porque isto substitui a reescrita do texto
///
/// O caminho antigo compilava cada `<script>` com `new Function` e, como um
/// programa novo não vê o topo do que o criou, qualificava cada nome livre para
/// `__G.<nome>` sobre o TEXTO antes de compilar. Isso é um resolvedor de escopo
/// escrito fora do compilador: não converge (sombreamento, destructuring, uma
/// regex que parece divisão) e compila um programa que a página não serviu.
///
/// Aqui o compilador resolve, porque é ele que sabe o que é uma ligação.
/// `evaluate_in_scope_with_receiver` compila contra um objeto de ambiente — o
/// `bag.object` que este módulo já mantém vivo, um por documento — e
/// `emit::page` faz as declarações de topo assentarem nele, que é o que a
/// ECMA-262 §16.1.7 diz de script code. O `window` vai como receiver, para que
/// `this` no topo de um `<script>` seja o `window`, como num browser.
///
/// # Porque o erro é engolido aqui
///
/// Um `<script>` partido não derruba a página — é o comportamento do browser, e
/// era o que o `try/catch` do `.ts` fazia. O que MUDA é que a falha deixa de ser
/// indistinguível de um prelude em falta: um `None` daqui é fonte que não
/// compilou, e não um nome do prelude que não existe.
extern "C" fn run(_e: u64, _t: u64, doc: u64, source: u64, window: u64, _c: u64) -> u64 {
    let Some(text) = entry::text_of(source) else {
        return int(0);
    };
    let object = object_for(handle(doc));
    let answered = entry::evaluate_in_scope_with_receiver(&text, object, window);
    // O ISOLAMENTO, e é aqui que tem de estar. Um `<script>` que lança não
    // derruba a página — é o que um browser faz — e o `try`/`catch` do `.ts`
    // que fazia isto já não está no caminho: o erro sai deste programa por um
    // canal lateral do motor, que um `catch` de TypeScript não observa.
    //
    // Medido no WhatsApp Web real: 71 scripts, e um `TypeError` de dentro de um
    // deles terminava o processo com a página por montar. Consumir aqui é o que
    // torna a falha DESTE script e não da página.
    let raised = entry::pending();
    if raised.is_some() {
        entry::take_thrown();
    }
    // O QUE falhou fica guardado, e isso não é só diagnóstico: um browser
    // imprime o erro de um `<script>` no console, e uma falha que não diz nada
    // é indistinguível de um script que não fez nada. Foi assim que o prelude
    // em falta passou dezassete dias por descobrir — ver
    // `docs/ui/page-script-bridge.md`.
    let message = match (&answered, &raised) {
        (_, Some((_, text))) => Some(text.clone()),
        // Compilou-se nada e não houve throw: fonte que o front end recusou.
        // Um `SyntaxError` de um `<script>` é ordinário numa página real, e
        // dizer "não compilou" é mais do que o silêncio de antes mesmo sem a
        // mensagem do parser, que não atravessa esta fronteira.
        (None, None) => Some("a fonte não compilou".to_owned()),
        (Some(_), None) => None,
    };
    if let Some(text) = message {
        let mut bags = locked();
        if let Some(bag) = bags.get_mut(&handle(doc)) {
            bag.last_error = Some(text);
        }
        return int(0);
    }
    int(1)
}

/// `DomScope.lastError(h)` — o que o último `<script>` que falhou disse, e
/// limpa-o. `""` quando nenhum falhou desde a última leitura.
extern "C" fn last_error(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let taken = locked().get_mut(&handle(doc)).and_then(|bag| bag.last_error.take());
    string(&taken.unwrap_or_default())
}

/// `DomScope.count(h)` — quantos globais este documento publicou.
extern "C" fn count(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let n = locked().get(&handle(doc)).map(|bag| bag.order.len()).unwrap_or(0);
    int(n as i64)
}

/// `DomScope.nameAt(h, n)` — o n-ésimo nome publicado (`""` fora de faixa).
extern "C" fn name_at(_e: u64, _t: u64, doc: u64, index: u64, _b: u64, _c: u64) -> u64 {
    let n = integer(index, -1);
    if n < 0 {
        return string("");
    }
    let name = locked()
        .get(&handle(doc))
        .and_then(|bag| bag.order.get(n as usize).cloned())
        .unwrap_or_default();
    string(&name)
}

/// `DomScope.get(h, name)` — o valor, `undefined` se não existe.
extern "C" fn get(_e: u64, _t: u64, doc: u64, name: u64, _b: u64, _c: u64) -> u64 {
    let name = text(name);
    // Nada publicado ainda: responder `undefined` sem CRIAR o saco. Uma leitura
    // de nome inexistente é o caso comum — a cadeia de escopo pergunta por todo
    // o nome livre antes de cair no objeto global — e não é razão para alocar
    // um objeto por documento que só lê.
    let Some(object) = locked().get(&handle(doc)).map(|bag| bag.object) else {
        return nothing();
    };
    entry::get_indexed(object, string(&name))
}

/// `DomScope.set(h, name, value)` — guarda o valor sob o nome.
extern "C" fn set(_e: u64, _t: u64, doc: u64, name: u64, value: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let name = text(name);
    let object = object_for(h);
    entry::set_indexed(object, string(&name), value, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    remember(h, &name);
    nothing()
}

/// `DomScope.has(h, name)` — `1` se o global existe.
///
/// `1`/`0` e não um booleano porque o `.ts` compara com `=== 0`, que é a forma
/// que o resto desta fronteira usa.
extern "C" fn has(_e: u64, _t: u64, doc: u64, name: u64, _b: u64, _c: u64) -> u64 {
    let name = text(name);
    let known = locked()
        .get(&handle(doc))
        .map(|bag| bag.known.contains(&name))
        .unwrap_or(false);
    int(i64::from(known))
}

/// `DomScope.drop(h)` — descarta o saco deste documento.
extern "C" fn drop_member(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    drop_scope(handle(doc));
    nothing()
}
