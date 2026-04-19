mod common;

use common::{make_manifest, pkgref, vrange};
use elu_resolver::error::ResolverError;
use elu_resolver::resolve;
use elu_resolver::source::OfflineSource;
use elu_resolver::types::RootRef;

/// Slice 11: OfflineSource resolves refs present in local refs; missing refs
/// produce NotInLocalStore.
#[tokio::test]
async fn offline_resolves_local_ref_and_errors_on_missing() {
    let mut src = OfflineSource::new();
    let m = make_manifest("acme", "thing", "1.2.3", vec![]);
    let h = elu_manifest::manifest_hash(&m);
    src.insert(m, h.clone());

    // Present locally → resolves.
    let root = RootRef {
        package: pkgref("acme", "thing"),
        version: vrange("^1.0"),
    };
    let resolution = resolve(&[root], &src, None, None).await.expect("resolve ok");
    assert_eq!(resolution.manifests.len(), 1);
    assert_eq!(resolution.manifests[0].hash, h);

    // Not present → error.
    let missing = RootRef {
        package: pkgref("acme", "missing"),
        version: vrange("^1.0"),
    };
    let err = resolve(&[missing], &src, None, None).await.expect_err("missing");
    assert!(
        matches!(err, ResolverError::NoMatch { .. } | ResolverError::NotInLocalStore { .. }),
        "expected NoMatch/NotInLocalStore, got {err:?}"
    );
}
