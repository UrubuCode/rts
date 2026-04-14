use std::time::{SystemTime, UNIX_EPOCH};

use crate::codegen::ometa::OmetaWriter;

#[test]
fn ometa_writer_serializes_correctly() {
    let mut writer = OmetaWriter::new("development", "/project/src");
    writer.add_location(0x12a3f, "index.ts", 42, 10);
    writer.add_function("_rts_foo", 0x12a00, 256, "index.ts", 40);

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rts_ometa_test_{stamp}.o"));

    writer.write_to(&path).expect("must write ometa file");
    let ometa_path = path.with_extension("ometa");
    let json = std::fs::read_to_string(&ometa_path).expect("must read ometa file");

    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"mode\": \"development\""));
    assert!(json.contains("\"0x12a3f\""));
    assert!(json.contains("\"line\": 42"));
    assert!(json.contains("\"_rts_foo\""));

    let _ = std::fs::remove_file(ometa_path);
}
