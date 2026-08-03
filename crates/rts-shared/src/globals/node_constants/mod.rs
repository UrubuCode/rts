//! `Node` — as constantes de `nodeType` do DOM (Web IDL `Node.ELEMENT_NODE` …).
//!
//! Um script de boot faz `if (n.nodeType !== Node.ELEMENT_NODE) return` antes de
//! tocar num nó; sem a classe o script inteiro morre em "unbound identifier
//! `Node`" — e com ele todo o resto do bootstrap, que nada tem a ver com isso.
//!
//! ## Por que em Rust
//!
//! Isto era um `class Node { static get … }` no prelude `.ts` do `rts-dom`.
//! Migrou para cá pelo padrão vigente (`docs/engine/architecture.md`):
//! `#[rtse::class]` declara, o símbolo é DERIVADO da assinatura Rust e o
//! `rts-symbol-baker` bakeia a tabela — nada escrito à mão.
//!
//! É constante numérica pura, então atravessa a borda Rust↔JS sem problema.
//! (Um objeto JS vivo NÃO atravessa — foi por isso que o `MutationObserver`,
//! que guarda um callback, continua no `.ts`.)
//!
//! Vive em `rts-shared` porque é superfície universal não-primordial: não faz
//! I/O, não depende de backend, e a árvore DOM em si é outro crate. Os valores
//! são os da spec e batem com o que `el.nodeType` devolve.

/// Um elemento (`<div>`) — o caso que os guardas de boot testam.
#[rtse::constant(global = "Node", value = "ELEMENT_NODE")]
pub const ELEMENT_NODE: f64 = 1.0;

/// Um atributo (legado; `Attr` não é mais um nó filho na spec atual).
#[rtse::constant(global = "Node", value = "ATTRIBUTE_NODE")]
pub const ATTRIBUTE_NODE: f64 = 2.0;

/// Um nó de texto.
#[rtse::constant(global = "Node", value = "TEXT_NODE")]
pub const TEXT_NODE: f64 = 3.0;

/// Uma seção CDATA (`<![CDATA[…]]>`, só em XML).
#[rtse::constant(global = "Node", value = "CDATA_SECTION_NODE")]
pub const CDATA_SECTION_NODE: f64 = 4.0;

/// Uma processing instruction (`<?xml-stylesheet …?>`).
#[rtse::constant(global = "Node", value = "PROCESSING_INSTRUCTION_NODE")]
pub const PROCESSING_INSTRUCTION_NODE: f64 = 7.0;

/// Um comentário (`<!-- … -->`).
#[rtse::constant(global = "Node", value = "COMMENT_NODE")]
pub const COMMENT_NODE: f64 = 8.0;

/// O próprio documento.
#[rtse::constant(global = "Node", value = "DOCUMENT_NODE")]
pub const DOCUMENT_NODE: f64 = 9.0;

/// O doctype (`<!DOCTYPE html>`).
#[rtse::constant(global = "Node", value = "DOCUMENT_TYPE_NODE")]
pub const DOCUMENT_TYPE_NODE: f64 = 10.0;

/// Um `DocumentFragment`.
#[rtse::constant(global = "Node", value = "DOCUMENT_FRAGMENT_NODE")]
pub const DOCUMENT_FRAGMENT_NODE: f64 = 11.0;

/// Registra a classe `Node` com as constantes de `nodeType`.
///
/// São `#[rtse::constant]` e não métodos estáticos: um script de página escreve
/// `Node.ELEMENT_NODE` como PROPRIEDADE, não como chamada. `MemberKind::Constant`
/// é exatamente isso no engine — um getter que o codegen chama onde o nome
/// aparece sem parênteses —, então a forma da spec é preservada.
pub fn register(e: &mut rts_engine::Engine) {
    e.class("Node")
        .doc("Node — nodeType constants (Web IDL).")
        .member(element_node_member())
        .member(attribute_node_member())
        .member(text_node_member())
        .member(cdata_section_node_member())
        .member(processing_instruction_node_member())
        .member(comment_node_member())
        .member(document_node_member())
        .member(document_type_node_member())
        .member(document_fragment_node_member())
        .done();
}
