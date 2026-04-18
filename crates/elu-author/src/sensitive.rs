use globset::{Glob, GlobSet, GlobSetBuilder};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SensitiveHit {
    pub path: String,
    pub pattern: String,
}

/// Patterns enumerated from the PRD acceptance criteria.
pub const SENSITIVE_PATTERNS: &[&str] = &[
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/id_rsa",
    "**/id_rsa.*",
    "**/id_ed25519",
    "**/id_ed25519.*",
    "**/.ssh/**",
    "**/.netrc",
    "**/.aws/credentials",
    "**/.git/**",
];

struct Matcher {
    set: GlobSet,
    patterns: Vec<String>,
}

impl Matcher {
    fn build() -> Self {
        let mut builder = GlobSetBuilder::new();
        let patterns: Vec<String> = SENSITIVE_PATTERNS.iter().map(|s| s.to_string()).collect();
        for p in &patterns {
            builder.add(Glob::new(p).expect("sensitive pattern must parse"));
        }
        let set = builder.build().expect("sensitive set must build");
        Self { set, patterns }
    }

    fn first_match(&self, path: &str) -> Option<String> {
        let idxs = self.set.matches(path);
        idxs.first().map(|&i| self.patterns[i].clone())
    }
}

pub fn scan_paths(paths: &[&str]) -> Vec<SensitiveHit> {
    let m = Matcher::build();
    let mut hits = Vec::new();
    for p in paths {
        if let Some(pat) = m.first_match(p) {
            hits.push(SensitiveHit {
                path: (*p).to_string(),
                pattern: pat,
            });
        }
    }
    hits
}
