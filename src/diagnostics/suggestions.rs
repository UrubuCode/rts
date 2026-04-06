pub fn suggest_for_message(message: &str) -> Option<&'static str> {
    if message.contains("duplicated type declaration") {
        return Some("Use a different type name or remove the duplicate declaration.");
    }

    if message.contains("unexpected token") {
        return Some("Check punctuation and type annotations near the highlighted span.");
    }

    None
}
