//! Descritor de um membro registrado (fn / const / método / getter / setter /
//! var) — o que o builder insere no [`Registry`](crate::Registry).

use crate::Sig;
use crate::abi::{Intrinsic, MemberFlags, MemberKind};

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
    /// Quando `Some`, o codegen emite IR Cranelift inline em vez de `call
    /// <symbol>` (sqrt, abs, min/max, …). Espelha `NamespaceMember.intrinsic`.
    pub intrinsic: Option<Intrinsic>,
}

impl Member {
    /// True se `name` casa o nome do membro ou um dos seus `aliases`.
    pub fn matches_name(&self, name: &str) -> bool {
        self.name == name || self.aliases.iter().any(|a| a == name)
    }
}
