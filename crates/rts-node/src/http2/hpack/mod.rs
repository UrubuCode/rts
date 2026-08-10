//! RFC 7541 — HPACK header compression, hand-rolled per `http2.md` §5.1's
//! instruction (no HPACK dependency is in `Cargo.toml`, and this module adds
//! none).
//!
//! # What is here
//!
//! [`static_table`] (the fixed 61-entry table, Appendix A), [`integer`] (the
//! integer primitive, §5.1), [`string`] (the string primitive, §5.2, over
//! [`huffman`]), [`dynamic_table`] (the per-direction eviction table, §4),
//! and this file's [`Decoder`]/[`Encoder`] tying them into the four header
//! field representations RFC 7541 §6 defines: indexed, literal with
//! incremental indexing, literal without indexing, literal never indexed.
//! [`Decoder`] also handles the dynamic-table-size-update representation
//! (§6.3), which is not a header field at all.
//!
//! # What is NOT here
//!
//! **Huffman encoding.** [`huffman`]'s module doc says why decode is still
//! mandatory. [`Encoder`] below always writes literal strings.
//!
//! **Encoder table selection.** [`Encoder::encode`] looks up the static
//! table and this session's own dynamic table for an exact or name match
//! before falling back to a full literal, so repeated header names/values
//! (the common case: `:method`, `content-type`, ...) compress — but it does
//! not attempt Huffman-coding the literal bytes it does write (see above),
//! so its output is a valid HPACK stream, not one representative of an
//! nghttp2/Node peer's minimum on-wire byte count.

pub mod dynamic_table;
pub mod huffman;
pub mod integer;
pub mod static_table;
pub mod string;

use dynamic_table::DynamicTable;

/// One decoded header field, in wire order — a decoder hands back a `Vec` of
/// these per `HEADERS` block, matching what `http2.md` calls `rawHeaders`.
pub struct HeaderField {
    /// The header name, already lowercase if the peer sent it that way —
    /// this module does not normalize case on decode (HTTP/2 forbids
    /// mixed-case names on the wire; see the module doc).
    pub name: String,
    /// The header value.
    pub value: String,
}

/// Decodes header blocks against one session direction's dynamic table.
pub struct Decoder {
    table: DynamicTable,
}

impl Decoder {
    /// A fresh decoder with a dynamic table budgeted at `max_dynamic_size`
    /// bytes (RFC 7541 §4.1 accounting — see [`dynamic_table`]).
    pub fn new(max_dynamic_size: usize) -> Self {
        Self { table: DynamicTable::new(max_dynamic_size) }
    }

    /// The peer's `SETTINGS_HEADER_TABLE_SIZE` changing what this decoder's
    /// own inbound table may hold — `http2.md` §4's "HPACK dynamic table"
    /// note: the inbound table's budget is the PEER's setting, never this
    /// side's `maxDeflateDynamicTableSize`.
    pub fn set_max_size(&mut self, max_size: usize) {
        self.table.set_max_size(max_size);
    }

    /// Decodes every header field in one `HEADERS`/`PUSH_PROMISE` block
    /// (already reassembled from any `CONTINUATION` frames — this module has
    /// none, see `frame.rs`'s doc). `None` on any malformed representation;
    /// HPACK errors are stream-fatal in the spec, mirrored here by refusing
    /// the whole block rather than returning a partial one.
    pub fn decode(&mut self, mut block: &[u8]) -> Option<Vec<HeaderField>> {
        let mut fields = Vec::new();
        while !block.is_empty() {
            let first = block[0];
            if first & 0x80 != 0 {
                // Indexed Header Field — RFC 7541 §6.1.
                let (index, consumed) = integer::decode(block, 7)?;
                let (name, value) = self.lookup(index as usize)?;
                fields.push(HeaderField { name, value });
                block = &block[consumed..];
            } else if first & 0x40 != 0 {
                // Literal Header Field with Incremental Indexing — §6.2.1.
                let (field, consumed) = self.decode_literal(block, 6)?;
                self.table.insert(field.name.clone(), field.value.clone());
                fields.push(field);
                block = &block[consumed..];
            } else if first & 0x20 != 0 {
                // Dynamic Table Size Update — §6.3. Not a header field.
                let (max_size, consumed) = integer::decode(block, 5)?;
                self.table.set_max_size(max_size as usize);
                block = &block[consumed..];
            } else {
                // Literal without indexing (0x0 prefix) or never indexed
                // (0x10 prefix) — §6.2.2/§6.2.3. Both a 4-bit index prefix;
                // neither touches the dynamic table, which is this crate's
                // only observable difference (`sensitiveHeaders` marking
                // "never indexed" on the wire has no reader-side effect).
                let (field, consumed) = self.decode_literal(block, 4)?;
                fields.push(field);
                block = &block[consumed..];
            }
        }
        Some(fields)
    }

    fn decode_literal(&self, block: &[u8], prefix_bits: u8) -> Option<(HeaderField, usize)> {
        let (index, mut consumed) = integer::decode(block, prefix_bits)?;
        let name = if index == 0 {
            let (text, used) = string::decode(&block[consumed..])?;
            consumed += used;
            text
        } else {
            self.lookup(index as usize)?.0
        };
        let (value, used) = string::decode(&block[consumed..])?;
        consumed += used;
        Some((HeaderField { name, value }, consumed))
    }

    fn lookup(&self, index: usize) -> Option<(String, String)> {
        if index == 0 {
            return None; // index 0 is never valid — RFC 7541 §6.1
        }
        if let Some((name, value)) = static_table::at(index) {
            return Some((name.to_owned(), value.to_owned()));
        }
        let dynamic_index = index - static_table::STATIC_TABLE.len();
        self.table.at(dynamic_index).map(|(n, v)| (n.to_owned(), v.to_owned()))
    }
}

/// Encodes header blocks against one session direction's dynamic table.
pub struct Encoder {
    table: DynamicTable,
}

impl Encoder {
    /// A fresh encoder with a dynamic table budgeted at `max_dynamic_size`
    /// bytes — this side's own `maxDeflateDynamicTableSize` (`http2.md`
    /// §4's "HPACK dynamic table" note: never the peer's setting).
    pub fn new(max_dynamic_size: usize) -> Self {
        Self { table: DynamicTable::new(max_dynamic_size) }
    }

    /// Shrinks or grows this encoder's own outbound table budget.
    pub fn set_max_size(&mut self, max_size: usize) {
        self.table.set_max_size(max_size);
    }

    /// Encodes one block. Always literal-with-incremental-indexing for a
    /// name/value neither table already has verbatim, so repeated requests
    /// on one session shrink over time exactly as HPACK intends — indexed
    /// representation when a prior field is an exact match.
    pub fn encode(&mut self, fields: &[HeaderField]) -> Vec<u8> {
        let mut out = Vec::new();
        for field in fields {
            if let Some(index) = static_table::find_exact(&field.name, &field.value) {
                out.extend(integer::encode(index as u64, 7, 0x80));
                continue;
            }
            if let Some(index) = self.find_dynamic_exact(&field.name, &field.value) {
                out.extend(integer::encode(index as u64, 7, 0x80));
                continue;
            }
            out.extend(self.encode_literal(field));
            self.table.insert(field.name.clone(), field.value.clone());
        }
        out
    }

    fn encode_literal(&self, field: &HeaderField) -> Vec<u8> {
        let mut out = match static_table::find_name(&field.name) {
            Some(index) => integer::encode(index as u64, 6, 0x40),
            None => match self.find_dynamic_name(&field.name) {
                Some(index) => integer::encode(index as u64, 6, 0x40),
                None => {
                    let mut prefix = integer::encode(0, 6, 0x40);
                    prefix.extend(string::encode(&field.name));
                    prefix
                }
            },
        };
        out.extend(string::encode(&field.value));
        out
    }

    fn find_dynamic_exact(&self, name: &str, value: &str) -> Option<usize> {
        (1..=self.table.len())
            .find(|&i| self.table.at(i) == Some((name, value)))
            .map(|i| i + static_table::STATIC_TABLE.len())
    }

    fn find_dynamic_name(&self, name: &str) -> Option<usize> {
        (1..=self.table.len())
            .find(|&i| self.table.at(i).map(|(n, _)| n) == Some(name))
            .map(|i| i + static_table::STATIC_TABLE.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_static_field_round_trips() {
        let mut decoder = Decoder::new(4096);
        // 0x82 = indexed, index 2 = (":method", "GET").
        let fields = decoder.decode(&[0x82]).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, ":method");
        assert_eq!(fields[0].value, "GET");
    }

    #[test]
    fn encoder_decoder_round_trip_with_dynamic_indexing() {
        let mut encoder = Encoder::new(4096);
        let mut decoder = Decoder::new(4096);
        let fields = vec![
            HeaderField { name: ":method".into(), value: "POST".into() },
            HeaderField { name: "x-custom".into(), value: "one".into() },
        ];
        let block = encoder.encode(&fields);
        let decoded = decoder.decode(&block).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].name, ":method");
        assert_eq!(decoded[0].value, "POST");
        assert_eq!(decoded[1].name, "x-custom");
        assert_eq!(decoded[1].value, "one");

        // A second block repeating the custom header should now hit the
        // encoder's dynamic table as an indexed field.
        let block2 = encoder.encode(&fields[1..]);
        assert_eq!(block2[0] & 0x80, 0x80, "expected an indexed representation on repeat");
        let decoded2 = decoder.decode(&block2).unwrap();
        assert_eq!(decoded2[0].name, "x-custom");
        assert_eq!(decoded2[0].value, "one");
    }

    #[test]
    fn dynamic_table_size_update_is_not_a_field() {
        let mut decoder = Decoder::new(4096);
        // 0x20 = dynamic table size update to 0.
        let fields = decoder.decode(&[0x20]).unwrap();
        assert!(fields.is_empty());
    }

    #[test]
    fn rfc_c_6_1_first_request_matches_spec_example() {
        // RFC 7541 C.6.1 — first request, huffman-coded, over a 256-byte
        // dynamic table: decode-only check since this encoder never emits
        // Huffman (module doc).
        let wire = [
            0x82, 0x86, 0x84, 0x41, 0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90,
            0xf4, 0xff,
        ];
        let mut decoder = Decoder::new(256);
        let fields = decoder.decode(&wire).unwrap();
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0].name, ":method");
        assert_eq!(fields[0].value, "GET");
        assert_eq!(fields[1].name, ":scheme");
        assert_eq!(fields[1].value, "http");
        assert_eq!(fields[2].name, ":path");
        assert_eq!(fields[2].value, "/");
        assert_eq!(fields[3].name, ":authority");
        assert_eq!(fields[3].value, "www.example.com");
    }
}
