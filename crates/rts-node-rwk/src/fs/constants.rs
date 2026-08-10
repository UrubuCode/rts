//! `fs.constants` — the POSIX flag/mode bit table, §2.3 of the reference doc.
//!
//! Plain numbers, not functions: nothing here reads across the value-API
//! borrow discipline the rest of this module has to mind, so [`install`]
//! builds and attaches the object directly rather than going through
//! [`super::mod`]'s `Provided`-based `make_namespace`.

use rts_core_rwk::entry::{Context, make_number, make_object, put_member};

/// Attaches `constants` onto an already-built `fs` namespace.
///
/// `context` here is the `&mut Context` [`super::namespace`] was handed
/// directly, at module-install time — not the ambient borrow a running
/// native reaches through `with_runtime`, so [`make_object`]/[`put_member`]
/// are called on it exactly as written, with no borrow to route around.
pub(super) fn install(context: &mut Context, namespace: u64) {
    let constants = build(context);
    put_member(context, namespace, "constants", constants);
}

fn build(context: &mut Context) -> u64 {
    let object = make_object(context);
    for (name, value) in ENTRIES {
        let number = make_number(*value as f64);
        put_member(context, object, name, number);
    }
    object
}

/// The whole table, `pub(crate)` because `node:constants` spreads it flat.
pub(crate) const ENTRIES: &[(&str, i64)] = &[
    ("F_OK", 0),
    ("R_OK", 4),
    ("W_OK", 2),
    ("X_OK", 1),
    ("O_RDONLY", 0),
    ("O_WRONLY", 1),
    ("O_RDWR", 2),
    ("O_CREAT", 64),
    ("O_EXCL", 128),
    ("O_NOCTTY", 256),
    ("O_TRUNC", 512),
    ("O_APPEND", 1024),
    ("O_DIRECTORY", 65536),
    ("O_NOATIME", 262144),
    ("O_NOFOLLOW", 131072),
    ("O_SYNC", 1052672),
    ("O_DSYNC", 4096),
    ("COPYFILE_EXCL", 1),
    ("COPYFILE_FICLONE", 2),
    ("COPYFILE_FICLONE_FORCE", 4),
    ("S_IFMT", 61440),
    ("S_IFREG", 32768),
    ("S_IFDIR", 16384),
    ("S_IFCHR", 8192),
    ("S_IFBLK", 24576),
    ("S_IFIFO", 4096),
    ("S_IFLNK", 40960),
    ("S_IFSOCK", 49152),
    ("S_IRWXU", 448),
    ("S_IRUSR", 256),
    ("S_IWUSR", 128),
    ("S_IXUSR", 64),
    ("S_IRWXG", 56),
    ("S_IRGRP", 32),
    ("S_IWGRP", 16),
    ("S_IXGRP", 8),
    ("S_IRWXO", 7),
    ("S_IROTH", 4),
    ("S_IWOTH", 2),
    ("S_IXOTH", 1),
    ("S_ISUID", 2048),
    ("S_ISGID", 1024),
    ("S_ISVTX", 512),
];
