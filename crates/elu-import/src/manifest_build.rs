use elu_manifest::validate::validate_stored;
use elu_manifest::{
    Dependency, Hook, Layer, Manifest, Metadata, Package, PackageRef, VersionSpec,
};
use elu_store::hash::{DiffId, ManifestHash};
use semver::Version;

/// Parameters for building an imported manifest.
pub struct ManifestParams {
    pub namespace: String,
    pub name: String,
    pub version: Version,
    pub kind: String,
    pub description: String,
    pub diff_id: DiffId,
    pub layer_size: u64,
    pub dependencies: Vec<(String, VersionSpec)>,
    pub metadata: toml::value::Table,
}

/// Build a stored-form Manifest from importer metadata.
///
/// `dependencies` is a list of `(package_ref_string, version_spec)` pairs
/// where the package_ref_string is "namespace/name".
pub fn build_manifest(params: ManifestParams) -> Result<Manifest, String> {
    let deps: Vec<Dependency> = params
        .dependencies
        .into_iter()
        .map(|(ref_str, version)| {
            let reference: PackageRef = ref_str.parse().map_err(|e: String| e)?;
            Ok(Dependency { reference, version })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let manifest = Manifest {
        schema: 1,
        package: Package {
            namespace: params.namespace,
            name: params.name,
            version: params.version,
            kind: params.kind,
            description: params.description,
            tags: vec![],
        },
        layers: vec![Layer {
            diff_id: Some(params.diff_id),
            size: Some(params.layer_size),
            name: None,
            include: vec![],
            exclude: vec![],
            strip: None,
            place: None,
            mode: None,
        }],
        dependencies: deps,
        hook: Hook::default(),
        metadata: Metadata(params.metadata),
    };

    validate_stored(&manifest).map_err(|e| e.to_string())?;
    Ok(manifest)
}

/// Store a manifest in the store and write a ref for it.
/// Returns the manifest hash.
pub fn store_manifest(
    manifest: &Manifest,
    store: &dyn elu_store::store::Store,
) -> Result<ManifestHash, crate::error::ImportError> {
    let canonical = elu_manifest::to_canonical_json(manifest);
    let hash = store.put_manifest(&canonical)?;
    store.put_ref(
        &manifest.package.namespace,
        &manifest.package.name,
        &manifest.package.version.to_string(),
        &hash,
    )?;
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_manifest_produces_valid_stored_manifest() {
        // Create a fake DiffId
        let diff_id: DiffId = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse()
            .unwrap();

        let mut meta = toml::value::Table::new();
        meta.insert(
            "apt".to_string(),
            toml::Value::Table({
                let mut t = toml::value::Table::new();
                t.insert("source".to_string(), toml::Value::String("test".into()));
                t
            }),
        );

        let manifest = build_manifest(ManifestParams {
            namespace: "debian".into(),
            name: "curl".into(),
            version: Version::new(8, 1, 2),
            kind: "debian".into(),
            description: "command line tool for transferring data with URL syntax".into(),
            diff_id,
            layer_size: 1024,
            dependencies: vec![(
                "debian/libcurl4".into(),
                VersionSpec::Any,
            )],
            metadata: meta,
        })
        .unwrap();

        assert_eq!(manifest.schema, 1);
        assert_eq!(manifest.package.namespace, "debian");
        assert_eq!(manifest.package.name, "curl");
        assert_eq!(manifest.package.kind, "debian");
        assert_eq!(manifest.layers.len(), 1);
        assert!(manifest.layers[0].diff_id.is_some());
        assert_eq!(manifest.layers[0].size, Some(1024));
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(
            manifest.dependencies[0].reference.as_str(),
            "debian/libcurl4"
        );

        // validate_stored should pass (already called inside build_manifest)
        validate_stored(&manifest).unwrap();
    }
}
