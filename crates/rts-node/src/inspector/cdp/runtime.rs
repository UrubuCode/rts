//! `Runtime.*` over the socket — `session.rs`'s dispatch, not a second one.
//!
//! The session builds its answers as runtime objects for `session.post`'s
//! callback; this turns the same answers into JSON with `JSON.stringify`'s own
//! entry point. The only method shaped differently is `Runtime.evaluate`,
//! whose socket form answers a `RemoteObject` (`type` beside `value`), which is
//! what the DevTools console reads.

use rts_core::entry;
use serde_json::{Value, json};

use super::super::session::{Answer, dispatch};
use super::Reply;

/// The one execution context there is, as DevTools needs to hear about it
/// before its console will evaluate anything.
fn context_created() -> (String, Value) {
    (
        "Runtime.executionContextCreated".to_owned(),
        json!({"context": {
            "id": 1, "origin": "", "name": "rts", "uniqueId": "rts-1",
            "auxData": {"isDefault": true, "type": "default", "frameId": "rts-frame"}
        }}),
    )
}

/// Runs `method` through the session dispatch and answers its JSON.
pub(super) fn call(method: &str, params: &Value) -> Result<Reply, String> {
    let params_text = params.to_string();
    let text = entry::with_runtime(|context| entry::make_string(context, &params_text));
    let params_value = entry::json_parse(text);
    let answered = entry::with_runtime(|context| dispatch(context, method, params_value));
    let mut reply = match answered {
        Answer::Value(object) => Reply::plain(to_json(object)),
        Answer::Refused(reason) => return Err(reason),
        Answer::Evaluate(source) => Reply::plain(evaluate(&source)),
    };
    if method == "Runtime.enable" {
        reply.events.push(context_created());
    }
    Ok(reply)
}

/// `Runtime.evaluate`'s `RemoteObject`: the `typeof`, and the value when it has
/// a JSON form. An object that has none is described by its text instead of
/// being dropped, which is what the console shows for it.
fn evaluate(source: &str) -> Value {
    let Some(value) = entry::evaluate(source) else {
        return json!({"result": {"type": "undefined"}});
    };
    let kind = entry::text_of(entry::type_of(value)).unwrap_or_else(|| "undefined".to_owned());
    let mut remote = json!({"type": kind});
    let described = to_json(value);
    if !described.is_null() || kind == "object" {
        remote["value"] = described.clone();
    }
    remote["description"] = Value::String(match &described {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    });
    json!({"result": remote})
}

/// A runtime value as JSON, through `JSON.stringify`. `null` when it has no
/// JSON form (`undefined`, a function).
fn to_json(value: u64) -> Value {
    let text = entry::text_of(entry::json_stringify(value));
    text.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null)
}
