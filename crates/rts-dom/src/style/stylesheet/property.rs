//! `@property` (lote P, §5.P item 4): regista `syntax`/`inherits`/
//! `initial-value` para uma custom property nomeada. O consumidor é
//! `vars::substitute` — um `var(--x)` sem declaração alcançável passava a
//! `""` (fallback vazio); com um `@property` registado, o `initial-value`
//! entra no lugar disso, e `inherits:false` corta a herança que o `--x`
//! normal sempre tem.
//!
//! `syntax` é guardado mas não VALIDADO contra o valor — este motor não tem
//! um parser de tipos de custom property (`<color>`, `<length>` …), e fingir
//! validação sem a ter seria pior que não a ter: um `@property` mal formado
//! passaria despercebido.

use std::collections::HashMap;

/// O `syntax` declarado — guardado como veio (`"<color>"`, `"*"`, um valor
/// separado por `|`…). Sem parser de tipos: ver a nota do módulo.
#[derive(Clone, PartialEq, Debug)]
pub struct PropertySyntax(pub String);

/// Uma `@property --nome { syntax: …; inherits: …; initial-value: … }` já
/// resolvida.
#[derive(Clone, PartialEq, Debug)]
pub struct RegisteredProperty {
    pub syntax: PropertySyntax,
    pub inherits: bool,
    /// `None` quando a regra não declara `initial-value` — a spec exige a
    /// declaração para um `syntax` diferente de `"*"`, mas este motor não
    /// recusa a regra por omiti-la (seria um segundo lugar a aplicar a
    /// validação de sintaxe que a nota do módulo já diz que não existe).
    pub initial_value: Option<String>,
}

/// Todas as `@property` de um `Stylesheet`, por nome (`--x`, com o prefixo).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct CustomPropertyRegistry {
    entries: HashMap<String, RegisteredProperty>,
}

impl CustomPropertyRegistry {
    pub fn get(&self, name: &str) -> Option<&RegisteredProperty> {
        self.entries.get(name)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `inherits: false` de uma `@property` — usado pela cascade para tirar a
    /// entrada do mapa herdado do pai ANTES de aplicar as declarações próprias
    /// (o valor do pai não deve nem aparecer para um filho sem declaração
    /// própria; sem isto o filho herdaria como qualquer custom property normal).
    pub fn inherits(&self, name: &str) -> bool {
        self.entries.get(name).map(|p| p.inherits).unwrap_or(true)
    }

    /// Preenche, em `vars`, o `initial-value` de toda `@property` que ainda não
    /// tenha entrada — o que a spec pede quando nem o elemento nem um
    /// antepassado declararam o custom property: `var(--x)` resolve para o
    /// inicial registado em vez de "" (o fallback vazio de sempre).
    pub fn seed_defaults(&self, vars: &mut HashMap<String, String>) {
        for (name, entry) in &self.entries {
            if let Some(initial) = &entry.initial_value {
                vars.entry(name.clone()).or_insert_with(|| initial.clone());
            }
        }
    }

    /// Regista (ou substitui — a última `@property` do mesmo nome vence, como
    /// qualquer at-rule) uma entrada a partir do PRELUDE (`--nome`) e do CORPO
    /// já tokenizado.
    pub fn register(&mut self, name: &str, block: &crate::style::syntax::BlockAst) {
        let name = name.trim();
        if !name.starts_with("--") || name.is_empty() {
            return; // nome inválido: `@property` sem `--` não é um custom property
        }
        let mut syntax = None;
        let mut inherits = None;
        let mut initial_value = None;
        for decl in block.declarations() {
            let value: String = decl
                .value
                .iter()
                .map(crate::style::syntax::ComponentValue::to_css_semantic)
                .collect();
            match decl.name.trim().to_ascii_lowercase().as_str() {
                "syntax" => syntax = Some(strip_quotes(value.trim())),
                "inherits" => inherits = Some(value.trim().eq_ignore_ascii_case("true")),
                "initial-value" => initial_value = Some(value.trim().to_string()),
                _ => {}
            }
        }
        // `syntax`/`inherits` são OBRIGATÓRIOS pela spec; uma regra sem os dois
        // não regista nada — melhor recusar que assumir um default errado
        // silenciosamente (a mesma disciplina de `style/inert.rs`).
        let (Some(syntax), Some(inherits)) = (syntax, inherits) else {
            return;
        };
        self.entries.insert(
            name.to_string(),
            RegisteredProperty {
                syntax: PropertySyntax(syntax),
                inherits,
                initial_value,
            },
        );
    }
}

/// Regista `@property --nome { … }` no `Stylesheet::properties`, se o at-rule
/// (já com o nome em minúsculas) for de facto um `@property`. Chamado de
/// `Stylesheet::append_css` — o mesmo ponto que baixa `@keyframes`, e pela
/// mesma razão: os dois são ESTRUTURAIS (não viram `Rule`) e o registo vive
/// no `Stylesheet`, não numa regra, porque `vars::substitute` consulta por
/// NOME e uma `@property` aplica-se ao documento inteiro, nunca só ao escopo
/// de um seletor. Vive aqui (não em `sheet.rs`, que já está no teto de 500
/// linhas) para não fazer esse ficheiro crescer.
pub(in crate::style::stylesheet) fn maybe_register(
    registry: &mut CustomPropertyRegistry,
    lower_name: &str,
    prelude: &[crate::style::syntax::ComponentValue],
    block: &crate::style::syntax::BlockAst,
) {
    if lower_name != "property" {
        return;
    }
    let name: String = prelude
        .iter()
        .map(crate::style::syntax::ComponentValue::to_css_semantic)
        .collect();
    registry.register(name.trim(), block);
}

impl super::Stylesheet {
    /// A `@property` registada para `name` (`--x`), se alguma folha anexada a
    /// declarou. Consultado por `vars::substitute` quando um `var(--x)` não
    /// resolve por herança nem por declaração.
    pub fn registered_property(&self, name: &str) -> Option<&RegisteredProperty> {
        self.properties.get(name)
    }

    /// O registo inteiro de `@property` — a cascade consulta-o para os dois
    /// efeitos que uma entrada única não cobre: `inherits:false` (que precisa
    /// de saber ANTES de herdar) e o `initial-value` de defaults ausentes.
    pub(crate) fn properties_registry(&self) -> &CustomPropertyRegistry {
        &self.properties
    }
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if (bytes[0] == b'"' && bytes[s.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[s.len() - 1] == b'\'')
        {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(css: &str) -> crate::style::syntax::BlockAst {
        let sheet = crate::style::syntax::StylesheetAst::parse(&format!("@property --x {{{css}}}"));
        for item in &sheet.items {
            if let crate::style::syntax::AstItem::AtRule { block: Some(b), .. } = item {
                return b.clone();
            }
        }
        panic!("no @property block parsed");
    }

    #[test]
    fn regista_syntax_inherits_e_initial_value() {
        let mut reg = CustomPropertyRegistry::default();
        reg.register(
            "--x",
            &block(r#"syntax: "<color>"; inherits: false; initial-value: red;"#),
        );
        let entry = reg.get("--x").expect("registada");
        assert_eq!(entry.syntax.0, "<color>");
        assert!(!entry.inherits);
        assert_eq!(entry.initial_value.as_deref(), Some("red"));
    }

    #[test]
    fn sem_syntax_ou_inherits_nao_regista() {
        let mut reg = CustomPropertyRegistry::default();
        reg.register("--y", &block("initial-value: 1px;"));
        assert!(reg.get("--y").is_none());
    }

    #[test]
    fn nome_sem_prefixo_e_ignorado() {
        let mut reg = CustomPropertyRegistry::default();
        reg.register("cor", &block(r#"syntax: "*"; inherits: true;"#));
        assert!(reg.get("cor").is_none());
    }
}
