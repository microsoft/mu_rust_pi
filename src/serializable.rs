pub mod serializable_hob;

use r_efi::efi::Guid;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub fn format_guid(guid: Guid) -> String {
    // We need this because refi::Guid has private fields
    // and we can't make it derive Serialize (can't modify efi::Guid directly)
    let (time_low, time_mid, time_hi_and_version, clk_seq_hi_res, clk_seq_low, node) = guid.as_fields();
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        time_low,
        time_mid,
        time_hi_and_version,
        clk_seq_hi_res,
        clk_seq_low,
        node[0],
        node[1],
        node[2],
        node[3],
        node[4],
        node[5]
    )
}

pub trait Interval: Clone + Ord {
    fn start(&self) -> u64;

    fn end(&self) -> u64;

    fn merge(&self, other: &Self) -> Self;

    fn length(&self) -> u64 {
        self.end() - self.start()
    }

    fn contains(&self, other: &Self) -> bool {
        self.start() <= other.start() && self.end() >= other.end()
    }

    /// Check if this interval overlaps with another one.
    /// ```ignore
    /// - [s] [o] - non overlapping
    /// - [s[]o] - overlapping
    /// - [s[o]] - overlapping
    /// - [o[s]] - overlapping
    /// - [o[]s] - overlapping
    /// - [o] [s] - non overlapping
    /// ```
    fn overlaps(&self, other: &Self) -> bool {
        self.start() < other.end() && other.start() < self.end()
    }

    /// Check if this interval is adjacent to another one.
    /// Adjacency means:
    /// ```ignore
    /// - [s][o] or [o][s] (end of one is exactly the start of the other)
    /// ```
    fn adjacent(&self, other: &Self) -> bool {
        self.end() == other.start() || other.end() == self.start()
    }

    fn try_merge(&self, other: &Self) -> Option<Self> {
        if self.overlaps(other) || self.adjacent(other) { Some(self.merge(other)) } else { None }
    }

    fn merge_intervals(intervals: &[&Self]) -> Vec<Self> {
        if intervals.is_empty() {
            return Vec::new();
        }

        let mut sorted = intervals.to_vec();
        sorted.sort();

        let mut result = vec![sorted[0].clone()];
        for current in sorted.into_iter().skip(1) {
            let last = result.last_mut().unwrap();
            if let Some(merged) = last.try_merge(current) {
                *last = merged;
            } else {
                result.push((*current).clone());
            }
        }

        result
    }
}

mod hex_format {
    use core::fmt::LowerHex;
    use serde::Deserialize;
    use serde::{self, Deserializer, Serializer};

    pub fn serialize<T, S>(num: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: LowerHex,
        S: Serializer,
    {
        serializer.serialize_str(&format!("0x{:x}", num))
    }

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

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStrRadix,
        D: Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        let s = s.strip_prefix("0x").ok_or_else(|| serde::de::Error::custom("Missing '0x' prefix"))?;
        T::from_str_radix_16(s).map_err(serde::de::Error::custom)
    }
}
