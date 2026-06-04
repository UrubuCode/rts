//! `Blob` e `File` global classes (#74/#75).
//!
//! Blob concatena partes (string/Uint8Array/Buffer/Blob) num buffer imutavel.
//! File estende Blob com `name` + `lastModified`. Instancia eh `Entry::Map`
//! com fields `bytes` (Buffer handle), `size`, e (File) `name`/`lastModified`.

pub mod abi;
pub mod instance;
