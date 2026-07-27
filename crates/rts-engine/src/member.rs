//! Descritor de um membro registrado (fn / const / método / getter / setter /
//! var) — o que o builder insere no [`Registry`](crate::Registry).

use crate::Sig;
use crate::abi::{MemberFlags, MemberKind};

/// Ponteiro para a implementação nativa de um membro. É o código `extern "C"`
/// real; dobra como **símbolo do JIT** (o loader injeta `(symbol, fn_ptr)` em
/// `JITBuilder::symbol`, e a relocação resolve a chamada para este ponteiro).
///
/// `*const u8` não é `Send`/`Sync`, mas estes ponteiros apontam para código
/// estático (do binário ou de uma `.dll`/`.so` mantida viva pelo registry) e
/// nunca são desreferenciados como dado — logo é seguro compartilhá-los entre
/// threads. O `unsafe impl` documenta essa invariante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FnPtr(pub *const u8);

// SAFETY: aponta para código estático imutável (ver doc acima).
unsafe impl Send for FnPtr {}
unsafe impl Sync for FnPtr {}

impl FnPtr {
    /// Endereço como inteiro (para registrar no símbolo do JIT, comparar, etc.).
    pub fn addr(self) -> usize {
        self.0 as usize
    }
}

/// EMITTER NATIVO de um membro: emite IR Cranelift no ponto da chamada em vez de
/// `call <symbol>`.
///
/// Recebe o `FunctionBuilder` da função que está sendo gerada e os argumentos já
/// baixados (escalares, na representação que o membro declara). Devolve o valor
/// resultante, ou `None` quando o emitter não consegue atender AQUELE site — o
/// motor então emite a chamada normal. Por isso um emitter só pode deixar um
/// site mais rápido, nunca quebrá-lo.
///
/// ## Por que é `fn` e não `Box<dyn Fn>`
///
/// Um ponteiro de função mantém o [`Member`] `Clone` + `Send`/`Sync` sem
/// alocação, e closures SEM CAPTURA (`|b, args| { … }`) coagem para `fn`
/// automaticamente — que é exatamente a forma de autoria pretendida. Um emitter
/// que precisasse capturar estado seria, por definição, dependente do programa,
/// e isso não pertence a um spec de membro.
///
/// ## Fronteira honesta do que dá para emitir
///
/// Computação sobre ESCALARES (aritmética, bits, comparação, conversão) vira IR.
/// Qualquer coisa que toque o heap — string, objeto, `Vec`, alocação — precisa da
/// HandleTable e do alocador, que não existem em IR; para essas, o emitter deve
/// devolver `None` (ou emitir ele mesmo a chamada). "Tudo em Cranelift" tem esse
/// teto, e ele é do modelo de memória, não do Cranelift.
///
/// ## Restrição de segurança (decisão do owner)
///
/// O emitter é superfície de SPEC/motor. Ele nunca é exposto ao userland: não
/// entra em `rts.d.ts`, não vira namespace chamável de TS. Código de usuário só
/// alcança o membro pelo nome JS normal.
pub type NativeEmit = fn(
    &mut cranelift_frontend::FunctionBuilder,
    &[cranelift_codegen::ir::Value],
) -> Option<cranelift_codegen::ir::Value>;

/// Binding de uma variável de módulo/global declarada via o builder.
/// `Const` = só leitura (READONLY); `Mutable` = `let`/`var` (read + write).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    Const,
    Mutable,
}

/// Um membro resolvível: nome JS, kind, assinatura, símbolo canônico, ponteiro
/// nativo, modifiers, aliases. Equivalente programático da `NamespaceMember`
/// antiga — mas dono dos seus dados (`String`/`Vec`, não `&'static`), porque o
/// registry é montado em runtime (no startup do `rts.exe` e ao carregar
/// módulos externos), não em `const`.
#[derive(Debug, Clone)]
pub struct Member {
    /// Nome JS-visível, ex.: `"print"`, `"indexOf"`.
    pub name: String,
    /// Função / constante / construtor / método / getter / setter / var.
    pub kind: MemberKind,
    /// Assinatura ABI (com o receiver no slot 0 para métodos de instância).
    pub sig: Sig,
    /// Símbolo canônico `__RTS_FN_...` — usado pela relocação do AOT e por
    /// diagnóstico. No JIT, o `fn_ptr` é o que resolve.
    pub symbol: String,
    /// Implementação nativa. Ver [`FnPtr`].
    pub fn_ptr: FnPtr,
    /// Modifiers (readonly / static / mutable).
    pub flags: MemberFlags,
    /// Nomes alternativos que resolvem para este membro (ex.: `toLocaleLowerCase`).
    pub aliases: Vec<String>,
    /// Último parâmetro lógico é variádico (`...args`).
    pub variadic: bool,
    /// Assinatura TS (para `rts.d.ts` / tooling). Pode ser vazia.
    pub ts_signature: String,
    /// Doc-comment, se houver.
    pub doc: String,
    /// Membro puro (sem I/O / estado mutável / não-determinismo) — elegível pra
    /// paralelização automática (silent-parallelism). Conservador: false é
    /// sempre seguro. Espelha `NamespaceMember.pure`.
    pub pure: bool,

    /// EMITTER NATIVO (ver [`NativeEmit`]): quando `Some`, o motor chama isto
    /// para emitir o corpo do membro como IR no ponto da chamada, em vez de
    /// `call <symbol>`.
    ///
    /// O `symbol`/`fn_ptr` continua registrado — quem não resolve o membro
    /// estaticamente (reflexão, FFI, um receiver não provado) ainda chama a
    /// implementação nativa. O emitter é um caminho rápido opcional, não a única
    /// definição do membro.
    pub emit: Option<NativeEmit>,

    /// The RETURN CLASS, as DATA rather than a `ts_signature` string to
    /// re-parse. `Some("StreamReader")` for a member whose Rust return type
    /// names a registered class (`-> StreamReader`, `-> Self`, `-> Option<Self>`,
    /// or `#[rtse::method(returns = "Class")]`) — set by the `rtse` macros at
    /// expansion time so a typo'd class name is a `rustc` unresolved-path error,
    /// not a silently-dropped string match. `None` when the return carries no
    /// class identity (void/scalar/string/plain `Handle`/`Vec<T>`/…). Consumers
    /// (`crates/rts-codegen-new/src/front/run/registry.rs`) prefer this field
    /// over parsing `ts_signature`, falling back to the string parse only for
    /// members that have not been migrated to declare it.
    pub ret_class: Option<String>,
}

impl Member {
    /// True se `name` casa o nome do membro ou um dos seus `aliases`.
    pub fn matches_name(&self, name: &str) -> bool {
        self.name == name || self.aliases.iter().any(|a| a == name)
    }
}
