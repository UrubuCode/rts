//! `ffi` namespace — interop com std::ffi (CStr/CString/OsStr/OsString).
//!
//! Permite TS lidar com strings C-terminadas (\0) e plataforma-OS, comuns
//! em interop com APIs nativas via `extern "C"`.

pub mod abi;
pub mod cstr;
pub mod cstring;
pub mod osstr;
