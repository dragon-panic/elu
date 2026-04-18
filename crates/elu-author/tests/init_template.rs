use std::collections::HashMap;
use std::fs;
use camino::Utf8Path;
use elu_author::init::{init_from_template, TemplateProvider};
use elu_author::report::Diagnostic;
use elu_author::tar_det::{build_deterministic_tar, TarEntry};
use elu_manifest::{to_canonical_json, Layer, Manifest, Package};
use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::hash::{DiffId, ManifestHash};
use elu_store::hasher::Hasher;
use elu_store::store::Store;
use semver::Version;

struct FakeProvider {
    manifest_bytes: Vec<u8>,
    blobs: HashMap<String, Vec<u8>>,
}

impl TemplateProvider for FakeProvider {
    fn fetch_manifest(
        &self,
        _namespace: &str,
        _name: &str,
        _version: Option<&str>,
    ) -> Result<Vec<u8>, Diagnostic> {
        Ok(self.manifest_bytes.clone())
    }

    fn fetch_blob(&self, diff_id: &DiffId) -> Result<Vec<u8>, Diagnostic> {
        self.blobs
            .get(&diff_id.to_string())
            .cloned()
            .ok_or_else(|| {
                Diagnostic::new(
                    "",
                    elu_author::report::ErrorCode::StoreError,
                    format!("blob {diff_id} not found"),
                )
            })
    }
}

fn fake_template_with_skill_md() -> FakeProvider {
    // Build a single-layer tar with one file.
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    let f = root.join("SKILL.md");
    fs::write(&f, b"# skeleton\n").unwrap();

    let entries = vec![TarEntry::file(f, "SKILL.md".into(), Some(0o644))];
    let tar_bytes = build_deterministic_tar(&entries).unwrap();

    // Compute diff_id (SHA-256 of uncompressed tar).
    let mut hasher = Hasher::new();
    hasher.update(&tar_bytes);
    let diff_id = DiffId(hasher.finalize());

    let manifest = Manifest {
        schema: 1,
        package: Package {
            namespace: "ox-community".into(),
            name: "rust-skill".into(),
            version: Version::parse("0.1.0").unwrap(),
            kind: "elu-template".into(),
            description: "scaffold for a rust skill".into(),
            tags: vec![],
        },
        layers: vec![Layer {
            diff_id: Some(diff_id.clone()),
            size: Some(tar_bytes.len() as u64),
            name: Some("seed".into()),
            include: vec![],
            exclude: vec![],
            strip: None,
            place: None,
            mode: None,
            follow_symlinks: false,
        }],
        dependencies: vec![],
        hook: Default::default(),
        metadata: Default::default(),
    };

    let mut blobs = HashMap::new();
    blobs.insert(diff_id.to_string(), tar_bytes);

    FakeProvider {
        manifest_bytes: to_canonical_json(&manifest),
        blobs,
    }
}

#[test]
fn template_fetch_unpacks_layer_files_into_target_dir() {
    let target = tempfile::tempdir().unwrap();
    let target_root = Utf8Path::from_path(target.path()).unwrap();

    let provider = fake_template_with_skill_md();
    init_from_template(target_root, "ox-community", "rust-skill", None, &provider).unwrap();

    let payload = target_root.join("SKILL.md");
    assert!(payload.as_std_path().exists(), "seed file unpacked");
    let contents = fs::read_to_string(payload.as_std_path()).unwrap();
    assert_eq!(contents, "# skeleton\n");
}

#[test]
fn template_verifies_diff_id_matches_blob_bytes() {
    // Corrupt blob: provider claims a diff_id but returns different bytes.
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(tmp.path()).unwrap();
    let f = root.join("X");
    fs::write(&f, b"a").unwrap();
    let entries = vec![TarEntry::file(f.clone(), "X".into(), Some(0o644))];
    let tar_bytes = build_deterministic_tar(&entries).unwrap();

    let mut h = Hasher::new();
    h.update(&tar_bytes);
    let advertised = DiffId(h.finalize());

    // Provider returns *different* bytes for the advertised id.
    let bogus = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let mut blobs = HashMap::new();
    blobs.insert(advertised.to_string(), bogus);

    let manifest = Manifest {
        schema: 1,
        package: Package {
            namespace: "x".into(),
            name: "y".into(),
            version: Version::parse("0.1.0").unwrap(),
            kind: "elu-template".into(),
            description: "t".into(),
            tags: vec![],
        },
        layers: vec![Layer {
            diff_id: Some(advertised),
            size: Some(tar_bytes.len() as u64),
            name: None,
            include: vec![],
            exclude: vec![],
            strip: None,
            place: None,
            mode: None,
            follow_symlinks: false,
        }],
        dependencies: vec![],
        hook: Default::default(),
        metadata: Default::default(),
    };
    let provider = FakeProvider {
        manifest_bytes: to_canonical_json(&manifest),
        blobs,
    };

    let target = tempfile::tempdir().unwrap();
    let target_root = Utf8Path::from_path(target.path()).unwrap();
    let err = init_from_template(target_root, "x", "y", None, &provider).unwrap_err();
    assert!(err.message.contains("diff_id") || err.message.to_lowercase().contains("mismatch"));
}

// Round-trip: we can also verify the contents of a pre-built layer are fetched;
// we don't need a real server here — the FakeProvider mirrors the production
// RegistryClient contract (fetch_manifest / fetch_blob).
#[test]
fn template_tolerates_store_roundtrip_of_bytes() {
    // Sanity check that the FsStore + tar combo the test uses is self-consistent,
    // guarding against regressions where the store's put/get would corrupt bytes.
    let tmp = tempfile::tempdir().unwrap();
    let store = FsStore::init_with_fsync(
        Utf8Path::from_path(tmp.path()).unwrap(),
        FsyncMode::Never,
    )
    .unwrap();
    let bytes = b"hello world".to_vec();
    let mh: ManifestHash = store.put_manifest(&bytes).unwrap();
    let out = store.get_manifest(&mh).unwrap().unwrap();
    assert_eq!(&out[..], &bytes[..]);
}
