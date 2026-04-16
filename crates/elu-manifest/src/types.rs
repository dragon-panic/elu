use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use elu_store::hash::{DiffId, ManifestHash};
use semver::{Version, VersionReq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub schema: u32,
    pub package: Package,

    #[serde(rename = "layer", default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,

    #[serde(rename = "dependency", default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,

    #[serde(default, skip_serializing_if = "Hook::is_empty")]
    pub hook: Hook,

    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Package {
    pub namespace: String,
    pub name: String,
    pub version: Version,
    pub kind: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Layer {
    // --- Stored form ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_id: Option<DiffId>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    // --- Common ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    // --- Source form ---
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl Layer {
    pub fn is_source_form(&self) -> bool {
        !self.include.is_empty()
    }
    pub fn is_stored_form(&self) -> bool {
        self.diff_id.is_some()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Dependency {
    #[serde(rename = "ref")]
    pub reference: PackageRef,
    #[serde(default = "default_version_spec")]
    pub version: VersionSpec,
}

fn default_version_spec() -> VersionSpec {
    VersionSpec::Any
}

/// A validated `namespace/name` reference.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackageRef(String);

impl PackageRef {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PackageRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn is_valid_segment(seg: &str) -> bool {
            !seg.is_empty()
                && seg.as_bytes()[0].is_ascii_alphanumeric()
                && seg.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        }
        match s.split_once('/') {
            Some((ns, name)) if is_valid_segment(ns) && is_valid_segment(name) => {
                Ok(PackageRef(s.to_string()))
            }
            _ => Err(format!("invalid package ref: {s}")),
        }
    }
}

impl Serialize for PackageRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PackageRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VersionSpec {
    Range(VersionReq),
    Pinned(ManifestHash),
    Any,
}

impl Serialize for VersionSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            VersionSpec::Range(req) => serializer.serialize_str(&req.to_string()),
            VersionSpec::Pinned(hash) => serializer.serialize_str(&hash.to_string()),
            VersionSpec::Any => serializer.serialize_str("*"),
        }
    }
}

impl<'de> Deserialize<'de> for VersionSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        // Try manifest hash first (starts with algo prefix)
        if s.starts_with("sha256:") {
            return s
                .parse::<ManifestHash>()
                .map(VersionSpec::Pinned)
                .map_err(serde::de::Error::custom);
        }
        if s == "*" {
            return Ok(VersionSpec::Any);
        }
        s.parse::<VersionReq>()
            .map(VersionSpec::Range)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Hook {
    #[serde(rename = "op", default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<HookOp>,
}

impl Hook {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// Closed set of declarative ops. v1 does NOT include `Run`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookOp {
    Chmod {
        paths: Vec<String>,
        mode: String,
    },
    Mkdir {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(default)]
        parents: bool,
    },
    Symlink {
        from: String,
        to: String,
        #[serde(default)]
        replace: bool,
    },
    Write {
        path: String,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(default)]
        replace: bool,
    },
    Template {
        input: String,
        output: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        vars: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
    },
    Copy {
        from: String,
        to: String,
    },
    Move {
        from: String,
        to: String,
    },
    Delete {
        paths: Vec<String>,
    },
    Index {
        root: String,
        output: String,
        #[serde(default = "default_index_format")]
        format: IndexFormat,
    },
    Patch {
        file: String,
        #[serde(flatten)]
        source: PatchSource,
        #[serde(default)]
        fuzz: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PatchSource {
    Inline { diff: String },
    File { from: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum IndexFormat {
    Sha256List,
    Json,
    Toml,
}

fn default_index_format() -> IndexFormat {
    IndexFormat::Sha256List
}

/// Free-form table. Preserved verbatim, never interpreted by elu.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Metadata(pub toml::value::Table);

impl Metadata {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
