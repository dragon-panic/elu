use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Algorithm tag in the canonical `<algo>:<hex>` prefix.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum HashAlgo {
    Sha256,
}

impl HashAlgo {
    pub fn as_str(&self) -> &'static str {
        match self {
            HashAlgo::Sha256 => "sha256",
        }
    }
}

impl fmt::Display for HashAlgo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A content hash with its algorithm tag.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Hash {
    algo: HashAlgo,
    bytes: [u8; 32],
}

impl Hash {
    pub fn new(algo: HashAlgo, bytes: [u8; 32]) -> Self {
        Self { algo, bytes }
    }

    pub fn algo(&self) -> HashAlgo {
        self.algo
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// First 2 hex characters — used for fan-out directory.
    pub fn prefix(&self) -> String {
        hex::encode(&self.bytes[..1])
    }

    /// Remaining 62 hex characters — used as filename.
    pub fn rest(&self) -> String {
        hex::encode(&self.bytes[1..])
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algo.as_str(), hex::encode(self.bytes))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HashParseError {
    #[error("missing ':' separator")]
    MissingSeparator,
    #[error("unknown algorithm: {0}")]
    UnknownAlgo(String),
    #[error("invalid hex: {0}")]
    InvalidHex(#[from] hex::FromHexError),
    #[error("wrong length: expected 32 bytes, got {0}")]
    WrongLength(usize),
}

impl FromStr for Hash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (algo_str, hex_str) = s.split_once(':').ok_or(HashParseError::MissingSeparator)?;
        let algo = match algo_str {
            "sha256" => HashAlgo::Sha256,
            other => return Err(HashParseError::UnknownAlgo(other.to_string())),
        };
        let decoded = hex::decode(hex_str)?;
        let bytes: [u8; 32] = decoded
            .try_into()
            .map_err(|v: Vec<u8>| HashParseError::WrongLength(v.len()))?;
        Ok(Hash { algo, bytes })
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Hash of the uncompressed tar bytes. Stable identity for a layer.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DiffId(pub Hash);

impl fmt::Display for DiffId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for DiffId {
    type Err = HashParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DiffId(s.parse()?))
    }
}

impl Serialize for DiffId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for DiffId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Hash of the bytes as stored on disk (possibly compressed). The CAS key.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct BlobId(pub Hash);

impl fmt::Display for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for BlobId {
    type Err = HashParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(BlobId(s.parse()?))
    }
}

impl Serialize for BlobId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for BlobId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Hash of a manifest (always uncompressed, so blob_id == diff_id).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct ManifestHash(pub Hash);

impl fmt::Display for ManifestHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ManifestHash {
    type Err = HashParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ManifestHash(s.parse()?))
    }
}

impl Serialize for ManifestHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ManifestHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
