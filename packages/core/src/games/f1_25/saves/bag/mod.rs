//! The game's property-bag stream (`Game_SaveData_WriteBag` / `Ne_BagCodec_ReadBag`): bytes to a typed tree,
//! to JSON, and back. The wire format is specified in `docs/reference/save-format.md`.

mod decode;
mod encode;
mod error;
mod scalar;
mod value;

#[cfg(test)]
mod tests;

pub use decode::{Limits, decode, decode_prefix, decode_with};
pub use encode::encode;
pub use error::{DecodeError, EncodeError, JsonError};
pub use scalar::{Float32, Float64, Hex, Hex16, Text};
pub use value::{Bag, Entry, List, Value};

pub const FORMAT_VERSION: u64 = 1;
pub const MAX_COUNT: u64 = 0xFFFF;

pub fn bag_to_json(bytes: &[u8]) -> Result<String, JsonError> {
    let bag = decode(bytes)?;
    Ok(serde_json::to_string_pretty(&bag)?)
}

pub fn bag_from_json(json: &str) -> Result<Vec<u8>, JsonError> {
    let bag: Bag = serde_json::from_str(json)?;
    Ok(encode(&bag)?)
}
