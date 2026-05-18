//! `EventTarget` + `Event` global classes (#63).
//!
//! EventTarget: addEventListener(type, fn), removeEventListener, dispatchEvent.
//! Event: new Event(type) com .type readonly.
//! Listeners chamados sincronamente em ordem de adicao, recebem o handle do
//! Event como primeiro arg.

pub mod abi;
pub mod instance;
