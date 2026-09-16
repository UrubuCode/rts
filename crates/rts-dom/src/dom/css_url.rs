//! URL computada de CSS: o parser guarda a referência crua, enquanto o CSSOM
//! devolve uma URL resolvida contra a folha que declarou o valor.
//!
//! O contrato recebe a base do chamador: para `<style>`/style inline é a URL
//! da página; uma folha externa pode passar a URL da própria folha quando a
//! proveniência das declarações for conservada no stylesheet.

use super::*;

impl Dom {
    pub fn computed_property_at(&self, id: NodeId, name: &str, base_url: &str) -> String {
        let value = self.computed_property(id, name);
        if base_url.is_empty()
            || !matches!(name.trim().to_ascii_lowercase().as_str(),
                "cursor" | "background-image" | "list-style-image" | "mask-image")
        {
            return value;
        }
        resolve_value_urls(&value, base_url)
    }
}

fn resolve_value_urls(value: &str, base: &str) -> String {
    let mut rest = value;
    let mut out = String::with_capacity(value.len() + base.len());
    while let Some(start) = rest.find("url(\"") {
        out.push_str(&rest[..start + 5]);
        let after = &rest[start + 5..];
        let Some(end) = after.find("\")") else {
            out.push_str(after);
            return out;
        };
        out.push_str(&resolve_reference(base, &after[..end]));
        out.push_str("\")");
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn resolve_reference(base: &str, reference: &str) -> String {
    if reference.is_empty() || has_scheme(reference) {
        return reference.to_string();
    }
    let Some(scheme_end) = base.find("://") else {
        return reference.to_string();
    };
    if reference.starts_with("//") {
        return format!("{}:{reference}", &base[..scheme_end]);
    }
    let authority_start = scheme_end + 3;
    let path_start = base[authority_start..]
        .find('/')
        .map(|n| authority_start + n)
        .unwrap_or(base.len());
    let origin = &base[..path_start];
    let base_path = if path_start == base.len() { "/" } else { &base[path_start..] };
    let base_path = base_path.split(['?', '#']).next().unwrap_or(base_path);
    if reference.starts_with('?') || reference.starts_with('#') {
        return format!("{origin}{base_path}{reference}");
    }
    let joined = if reference.starts_with('/') {
        reference.to_string()
    } else {
        let dir = &base_path[..base_path.rfind('/').unwrap_or(0) + 1];
        format!("{dir}{reference}")
    };
    let suffix_at = joined.find(['?', '#']).unwrap_or(joined.len());
    format!("{origin}{}{}", normalize_path(&joined[..suffix_at]), &joined[suffix_at..])
}

fn has_scheme(reference: &str) -> bool {
    let Some(colon) = reference.find(':') else { return false };
    colon > 0 && reference[..colon]
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn normalize_path(path: &str) -> String {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => { parts.pop(); }
            _ => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_absolutas_e_relativas_preservam_origem_e_fragmento() {
        let base = "https://site.test/a/b/page.html";
        assert_eq!(resolve_reference(base, "../x.png"), "https://site.test/a/x.png");
        assert_eq!(resolve_reference(base, "/x.png"), "https://site.test/x.png");
        assert_eq!(resolve_reference(base, "#icone"), "https://site.test/a/b/page.html#icone");
        assert_eq!(resolve_reference(base, "data:image/png;base64,AA"), "data:image/png;base64,AA");
        assert_eq!(resolve_reference(base, "//cdn.test/x.png"), "https://cdn.test/x.png");
    }
}
