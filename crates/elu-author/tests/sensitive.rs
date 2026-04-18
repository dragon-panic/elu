use elu_author::sensitive::scan_paths;

#[test]
fn detects_env_files() {
    let hits = scan_paths(&["config/.env", "config/.env.local", "src/lib.rs"]);
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert!(paths.contains(&"config/.env"));
    assert!(paths.contains(&"config/.env.local"));
    assert!(!paths.contains(&"src/lib.rs"));
}

#[test]
fn detects_pem_and_keys() {
    let hits = scan_paths(&["certs/server.pem", "certs/private.key", "id_rsa", "id_ed25519.pub"]);
    assert_eq!(hits.len(), 4);
}

#[test]
fn detects_ssh_and_netrc_and_git() {
    let hits = scan_paths(&[
        ".ssh/config",
        "home/.netrc",
        ".git/config",
        ".git/objects/ab/cdef",
    ]);
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert!(paths.contains(&".ssh/config"));
    assert!(paths.contains(&"home/.netrc"));
    assert!(paths.contains(&".git/config"));
    assert!(paths.contains(&".git/objects/ab/cdef"));
}

#[test]
fn benign_files_not_flagged() {
    let hits = scan_paths(&["README.md", "src/main.rs", "Cargo.toml", "bin/hello"]);
    assert!(hits.is_empty());
}

#[test]
fn hit_reports_matched_pattern() {
    let hits = scan_paths(&[".env"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, ".env");
    assert!(!hits[0].pattern.is_empty());
}
