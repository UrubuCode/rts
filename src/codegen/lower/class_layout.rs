//! Computacao do layout nativo de classes (passo 2 da #147).
//!
//! Funcao pura: dada uma `ClassMeta` e o layout do parent (se houver),
//! produz um `ClassLayout` com offsets fixos para cada field. So classes
//! 100% tipadas (sem getter/setter dinamico) sao elegiveis — caso
//! contrario retorna `None` e o codegen continua usando o caminho
//! `Map`-based atual.
//!
//! Layout: cada slot ocupa 8 bytes. Slot 0 (`offset=0`) reservado para
//! o tag `__rts_class` (handle de string com o nome da classe), por isso
//! a primeira classe da hierarquia comeca seus fields em offset 8.

use super::ctx::{ClassLayout, ClassMeta, FieldSlot, ValTy};

const SLOT_SIZE: u32 = 8;
const TAG_SIZE: u32 = 8;

/// Verifica se a classe é elegivel para layout nativo.
///
/// Criterios (conservadores nesta fase):
/// - todos os fields declarados em `field_class_names` precisam ter
///   tipo conhecido em `field_types`
/// - nao ha getters nem setters dinamicos
fn is_eligible(meta: &ClassMeta) -> bool {
    if !meta.getters.is_empty() || !meta.setters.is_empty() {
        return false;
    }
    for name in meta.field_class_names.keys() {
        if !meta.field_types.contains_key(name) {
            return false;
        }
    }
    true
}

/// Calcula o layout de `meta`, herdando o do `parent` quando presente.
///
/// Retorna `None` quando a classe nao é elegivel (mantém o caminho atual).
/// Quando o parent existe mas nao tem layout, esta classe tambem fica
/// inelegivel — todos os ancestrais precisam estar tipados para que os
/// offsets sejam consistentes.
pub fn compute_layout(meta: &ClassMeta, parent: Option<&ClassLayout>) -> Option<ClassLayout> {
    if !is_eligible(meta) {
        return None;
    }
    if meta.super_class.is_some() && parent.is_none() {
        return None;
    }

    // Inicio: campos comecam logo apos o parent (que ja inclui o tag),
    // ou apos o tag quando nao ha parent.
    let parent_size = parent.map(|p| p.size_bytes).unwrap_or(TAG_SIZE);

    // Coleta os fields *desta* classe em ordem estavel pelos nomes
    // declarados em `field_class_names`. Ignora entradas private
    // sintaticamente marcadas com `#` que nao tenham nome em
    // `field_class_names` (caso TS-only).
    let mut field_names: Vec<&String> = meta.field_class_names.keys().collect();
    field_names.sort();

    let mut fields = Vec::with_capacity(field_names.len());
    let mut cursor = parent_size;
    for name in field_names {
        let ty = *meta.field_types.get(name)?;
        let is_handle = matches!(ty, ValTy::Handle);
        fields.push(FieldSlot {
            name: name.clone(),
            offset: cursor,
            ty,
            is_handle,
        });
        cursor += SLOT_SIZE;
    }

    Some(ClassLayout {
        fields,
        size_bytes: cursor,
        parent_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn meta(name: &str) -> ClassMeta {
        ClassMeta {
            name: name.to_string(),
            super_class: None,
            methods: Vec::new(),
            field_types: HashMap::new(),
            field_class_names: HashMap::new(),
            static_methods: Vec::new(),
            static_fields: Vec::new(),
            getters: Vec::new(),
            setters: Vec::new(),
            has_constructor: false,
            readonly_fields: HashSet::new(),
            member_visibility: HashMap::new(),
            is_abstract: false,
            abstract_methods: HashSet::new(),
            layout: None,
        }
    }

    #[test]
    fn simple_class_layout() {
        let mut m = meta("Point");
        m.field_types.insert("x".to_string(), ValTy::F64);
        m.field_class_names.insert("x".to_string(), "f64".to_string());
        m.field_types.insert("y".to_string(), ValTy::F64);
        m.field_class_names.insert("y".to_string(), "f64".to_string());

        let layout = compute_layout(&m, None).expect("eligivel");
        assert_eq!(layout.parent_size, 8); // tag slot
        assert_eq!(layout.fields.len(), 2);
        // ordem alfabetica: x, y
        assert_eq!(layout.fields[0].name, "x");
        assert_eq!(layout.fields[0].offset, 8);
        assert_eq!(layout.fields[1].name, "y");
        assert_eq!(layout.fields[1].offset, 16);
        assert_eq!(layout.size_bytes, 24);
    }

    #[test]
    fn extends_inherits_offsets() {
        let mut parent = meta("Base");
        parent.field_types.insert("a".to_string(), ValTy::I64);
        parent
            .field_class_names
            .insert("a".to_string(), "i64".to_string());
        let parent_layout = compute_layout(&parent, None).expect("ok");
        assert_eq!(parent_layout.size_bytes, 16);

        let mut child = meta("Child");
        child.super_class = Some("Base".to_string());
        child.field_types.insert("b".to_string(), ValTy::I64);
        child
            .field_class_names
            .insert("b".to_string(), "i64".to_string());

        let child_layout = compute_layout(&child, Some(&parent_layout)).expect("ok");
        assert_eq!(child_layout.parent_size, 16);
        assert_eq!(child_layout.fields.len(), 1);
        assert_eq!(child_layout.fields[0].name, "b");
        assert_eq!(child_layout.fields[0].offset, 16);
        assert_eq!(child_layout.size_bytes, 24);
    }

    #[test]
    fn handle_field_marked() {
        let mut m = meta("S");
        m.field_types.insert("name".to_string(), ValTy::Handle);
        m.field_class_names
            .insert("name".to_string(), "string".to_string());
        let layout = compute_layout(&m, None).expect("ok");
        assert!(layout.fields[0].is_handle);
    }

    #[test]
    fn dynamic_getter_disqualifies() {
        let mut m = meta("D");
        m.field_types.insert("x".to_string(), ValTy::I64);
        m.field_class_names.insert("x".to_string(), "i64".to_string());
        m.getters.push("y".to_string());
        assert!(compute_layout(&m, None).is_none());
    }

    #[test]
    fn missing_parent_layout_disqualifies() {
        let mut m = meta("Sub");
        m.super_class = Some("Missing".to_string());
        let res = compute_layout(&m, None);
        assert!(res.is_none());
    }
}
