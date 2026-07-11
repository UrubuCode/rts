//! node:zlib — the synchronous compress/decompress cores. flate2 (miniz_oxide,
//! pure Rust) for zlib/gzip/raw-deflate; the brotli crate for Brotli. All are
//! `bytes -> bytes`; a decode error surfaces as `Err` (a thrown Error at the JS
//! boundary), never a panic.

use std::io::{Read, Write};

use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use flate2::write::{DeflateEncoder, GzEncoder, ZlibEncoder};
use flate2::Compression;

/// `deflateSync` — zlib-wrapped deflate.
pub fn deflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).map_err(err)?;
    e.finish().map_err(err)
}

/// `inflateSync` — inflate a zlib stream.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    ZlibDecoder::new(data).read_to_end(&mut out).map_err(err)?;
    Ok(out)
}

/// `deflateRawSync` — headerless deflate.
pub fn deflate_raw(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).map_err(err)?;
    e.finish().map_err(err)
}

/// `inflateRawSync` — inflate a headerless deflate stream.
pub fn inflate_raw(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    DeflateDecoder::new(data).read_to_end(&mut out).map_err(err)?;
    Ok(out)
}

/// `gzipSync`.
pub fn gzip(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).map_err(err)?;
    e.finish().map_err(err)
}

/// `gunzipSync`.
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    GzDecoder::new(data).read_to_end(&mut out).map_err(err)?;
    Ok(out)
}

/// `unzipSync` — auto-detect gzip (magic `1f 8b`) vs zlib.
pub fn unzip(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        gunzip(data)
    } else {
        inflate(data)
    }
}

/// `brotliCompressSync` — quality 11, lgwin 22 (Node's defaults).
pub fn brotli_compress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut w = brotli::CompressorWriter::new(&mut out, 4096, 11, 22);
        w.write_all(data).map_err(err)?;
    }
    Ok(out)
}

/// `brotliDecompressSync`.
pub fn brotli_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    brotli::Decompressor::new(data, 4096)
        .read_to_end(&mut out)
        .map_err(err)?;
    Ok(out)
}

fn err(e: std::io::Error) -> String {
    e.to_string()
}

/// `zlib.crc32(data[, value])` — the standard reflected CRC-32 (IEEE 802.3,
/// polynomial 0xEDB88320), continuing from `prev` (0 for a fresh checksum).
pub fn crc32(data: &[u8], prev: u32) -> u32 {
    let mut crc = !prev;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
