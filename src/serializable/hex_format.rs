use alloc::format;
use core::fmt::LowerHex;
use serde::Deserialize;
use serde::{self, Deserializer, Serializer};

/// Serialize a number as a hex string with "0x" prefix.
pub fn serialize<T, S>(num: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: LowerHex,
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{:x}", num))
}

/// Trait to parse a number from a hex string (with or without "0x" prefix).
pub trait FromStrRadix {
    fn from_str_radix_16(src: &str) -> Result<Self, &'static str>
    where
        Self: Sized;
}

impl FromStrRadix for u8 {
    fn from_str_radix_16(src: &str) -> Result<Self, &'static str> {
        u8::from_str_radix(src, 16).map_err(|_| "Invalid hex u8")
    }
}

impl FromStrRadix for u16 {
    fn from_str_radix_16(src: &str) -> Result<Self, &'static str> {
        u16::from_str_radix(src, 16).map_err(|_| "Invalid hex u16")
    }
}

impl FromStrRadix for u32 {
    fn from_str_radix_16(src: &str) -> Result<Self, &'static str> {
        u32::from_str_radix(src, 16).map_err(|_| "Invalid hex u32")
    }
}

impl FromStrRadix for u64 {
    fn from_str_radix_16(src: &str) -> Result<Self, &'static str> {
        u64::from_str_radix(src, 16).map_err(|_| "Invalid hex u64")
    }
}

impl FromStrRadix for usize {
    fn from_str_radix_16(src: &str) -> Result<Self, &'static str> {
        usize::from_str_radix(src, 16).map_err(|_| "Invalid hex usize")
    }
}

/// Deserialize a number from a hex string with "0x" prefix.
pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStrRadix,
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    let s = s.strip_prefix("0x").ok_or_else(|| serde::de::Error::custom("Missing '0x' prefix"))?;
    T::from_str_radix_16(s).map_err(serde::de::Error::custom)
}
