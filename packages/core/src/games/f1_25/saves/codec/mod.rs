//! The outer layers of F1 25's profile save: the data file (header, HMAC-SHA256, AES-256-CBC, zstd)
//! and the 416-byte index (AES-256-ECB).

use aes::Aes256;
use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use sha2::{Digest, Sha256};
use thiserror::Error;

mod mt;

pub const MAGIC: [u8; 4] = [0x65, 0xef, 0x00, 0x8a];
pub const HEADER_LEN: usize = 8;
pub const MAC_LEN: usize = 32;
pub const IV_LEN: usize = 16;
pub const BODY_OFFSET: usize = HEADER_LEN + MAC_LEN + IV_LEN;
pub const STREAM_HEADER: [u8; 16] = [5, 0, 0, 0, 16, 0, 0, 0, 4, 0, 0, 0, 0x01, 0x08, 0x65, 0x15];
pub const INDEX_LEN: usize = 416;

const SIZE_XOR: u32 = 0xc361_5f4e;
const DATA_KEY_SEED: u32 = 0x0808_6283;
const INDEX_KEY_SEED: u32 = 0x178b_102d;
const OBFUSCATION: [u8; 12] = [
    0xef, 0xa1, 0x2f, 0x8a, 0x4e, 0x5f, 0x61, 0xc3, 0xf1, 0x18, 0xcd, 0xad,
];

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("the save is shorter than its header, MAC and IV")]
    Short,
    #[error("the save does not start with the profile magic")]
    BadMagic,
    #[error("the size in the header is {header} but the file is {actual} bytes")]
    BadSize { header: u32, actual: usize },
    #[error("the encrypted body is not a whole number of blocks")]
    Unaligned,
    #[error("the HMAC does not match")]
    BadMac,
    #[error("the padding is invalid")]
    BadPadding,
    #[error("the decrypted body does not start with the stream header")]
    BadStreamHeader,
    #[error("zstd: {0}")]
    Zstd(String),
    #[error("the index is not a whole number of blocks")]
    BadIndexLength,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedData {
    pub iv: [u8; IV_LEN],
    pub frame: Vec<u8>,
    pub stream: Vec<u8>,
}

fn data_key() -> [u8; 32] {
    let mut mt = mt::Mt19937::new(DATA_KEY_SEED);
    let mut key = [0u8; 32];
    for chunk in key.chunks_exact_mut(4) {
        chunk.copy_from_slice(&mt.next_u32().to_le_bytes());
    }
    key
}

fn index_key() -> [u8; 32] {
    let mut mt = mt::Mt19937::new(INDEX_KEY_SEED);
    let mut digits = [0u8; 64];
    for digit in &mut digits {
        *digit = (mt.next_u32() % 15) as u8;
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = (digits[2 * i] << 4) | digits[2 * i + 1];
    }
    key
}

fn hmac_sha256(key: &[u8; 32], parts: &[&[u8]]) -> [u8; 32] {
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..32 {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    for part in parts {
        inner.update(part);
    }
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn obfuscate(bytes: &mut [u8]) {
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte ^= OBFUSCATION[i % OBFUSCATION.len()];
    }
}

fn cbc_decrypt(key: &[u8; 32], iv: &[u8; IV_LEN], data: &mut [u8]) {
    let cipher = Aes256::new(&(*key).into());
    let mut prev = *iv;
    for block in data.chunks_exact_mut(16) {
        let cur: [u8; 16] = block.try_into().expect("16 bytes");
        cipher.decrypt_block(block.try_into().expect("16 bytes"));
        for (b, p) in block.iter_mut().zip(prev) {
            *b ^= p;
        }
        prev = cur;
    }
}

fn cbc_encrypt(key: &[u8; 32], iv: &[u8; IV_LEN], data: &mut [u8]) {
    let cipher = Aes256::new(&(*key).into());
    let mut prev = *iv;
    for block in data.chunks_exact_mut(16) {
        for (b, p) in block.iter_mut().zip(prev) {
            *b ^= p;
        }
        cipher.encrypt_block(block.try_into().expect("16 bytes"));
        prev = block.try_into().expect("16 bytes");
    }
}

pub fn decode_data(file: &[u8]) -> Result<DecodedData, CodecError> {
    if file.len() < BODY_OFFSET + 16 {
        return Err(CodecError::Short);
    }
    if file[..4] != MAGIC {
        return Err(CodecError::BadMagic);
    }
    let header = u32::from_le_bytes(file[4..8].try_into().expect("4 bytes")) ^ SIZE_XOR;
    if header as usize != file.len() {
        return Err(CodecError::BadSize {
            header,
            actual: file.len(),
        });
    }
    let mut mac: [u8; MAC_LEN] = file[HEADER_LEN..HEADER_LEN + MAC_LEN]
        .try_into()
        .expect("32 bytes");
    obfuscate(&mut mac);
    let mut iv: [u8; IV_LEN] = file[HEADER_LEN + MAC_LEN..BODY_OFFSET]
        .try_into()
        .expect("16 bytes");
    obfuscate(&mut iv);
    let body = &file[BODY_OFFSET..];
    if !body.len().is_multiple_of(16) {
        return Err(CodecError::Unaligned);
    }
    let key = data_key();
    if hmac_sha256(&key, &[&iv, body]) != mac {
        return Err(CodecError::BadMac);
    }
    let mut plain = body.to_vec();
    cbc_decrypt(&key, &iv, &mut plain);
    let pad = *plain.last().ok_or(CodecError::Short)? as usize;
    if pad == 0
        || pad > 16
        || pad > plain.len()
        || plain[plain.len() - pad..].iter().any(|b| *b as usize != pad)
    {
        return Err(CodecError::BadPadding);
    }
    plain.truncate(plain.len() - pad);
    if plain.len() < STREAM_HEADER.len() || plain[..16] != STREAM_HEADER {
        return Err(CodecError::BadStreamHeader);
    }
    let frame = plain[16..].to_vec();
    let stream = zstd::stream::decode_all(frame.as_slice()).map_err(|e| CodecError::Zstd(e.to_string()))?;
    Ok(DecodedData { iv, frame, stream })
}

pub fn seal_frame(frame: &[u8], iv: [u8; IV_LEN]) -> Vec<u8> {
    let mut plain = Vec::with_capacity(STREAM_HEADER.len() + frame.len() + 16);
    plain.extend_from_slice(&STREAM_HEADER);
    plain.extend_from_slice(frame);
    let pad = 16 - plain.len() % 16;
    plain.resize(plain.len() + pad, pad as u8);
    let key = data_key();
    cbc_encrypt(&key, &iv, &mut plain);
    let mac = hmac_sha256(&key, &[&iv, &plain]);
    let total = BODY_OFFSET + plain.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&((total as u32) ^ SIZE_XOR).to_le_bytes());
    let mut mac = mac;
    obfuscate(&mut mac);
    out.extend_from_slice(&mac);
    let mut iv = iv;
    obfuscate(&mut iv);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&plain);
    out
}

pub fn encode_data(stream: &[u8], iv: [u8; IV_LEN], level: i32) -> Result<Vec<u8>, CodecError> {
    let frame = zstd::bulk::compress(stream, level).map_err(|e| CodecError::Zstd(e.to_string()))?;
    Ok(seal_frame(&frame, iv))
}

pub fn decrypt_index(index: &[u8]) -> Result<Vec<u8>, CodecError> {
    if !index.len().is_multiple_of(16) {
        return Err(CodecError::BadIndexLength);
    }
    let cipher = Aes256::new(&index_key().into());
    let mut out = index.to_vec();
    for block in out.chunks_exact_mut(16) {
        cipher.decrypt_block(block.try_into().expect("16 bytes"));
    }
    Ok(out)
}

pub fn encrypt_index(plain: &[u8]) -> Result<Vec<u8>, CodecError> {
    if !plain.len().is_multiple_of(16) {
        return Err(CodecError::BadIndexLength);
    }
    let cipher = Aes256::new(&index_key().into());
    let mut out = plain.to_vec();
    for block in out.chunks_exact_mut(16) {
        cipher.encrypt_block(block.try_into().expect("16 bytes"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
