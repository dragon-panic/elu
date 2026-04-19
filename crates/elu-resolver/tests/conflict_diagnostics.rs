mod common;

use common::{InMemorySource, dep, make_manifest, pkgref, vrange};
use elu_resolver::error::ResolverError;
use elu_resolver::resolve;
use elu_resolver::types::RootRef;

/// Slice 7: when two roots' transitive deps require the same package at
/// incompatible versions, the error names every offending chain.
#[tokio::test]
async fn conflict_lists_every_offending_chain() {
    let mut src = InMemorySource::new();

    // c@1 and c@2 both exist
    src.add(make_manifest("acme", "c", "1.0.0", vec![]));
    src.add(make_manifest("acme", "c", "2.0.0", vec![]));

    // a@1 depends on c@^1; b@1 depends on c@^2
    src.add(make_manifest(
        "acme",
        "a",
        "1.0.0",
        vec![dep("acme", "c", vrange("^1"))],
    ));
    src.add(make_manifest(
        "acme",
        "b",
        "1.0.0",
        vec![dep("acme", "c", vrange("^2"))],
    ));

    let roots = vec![
        RootRef {
            package: pkgref("acme", "a"),
            version: vrange("^1"),
        },
        RootRef {
            package: pkgref("acme", "b"),
            version: vrange("^1"),
        },
    ];

    let err = resolve(&roots, &src, None, None).await.expect_err("conflict");
    match err {
        ResolverError::Conflict { package, chains } => {
            assert_eq!(package, pkgref("acme", "c"));
            assert_eq!(chains.len(), 2, "two chains: one from a, one from b");
            let rendered: Vec<String> = chains.iter().map(|(c, _)| c.to_string()).collect();
            assert!(
                rendered.iter().any(|s| s.contains("acme/a") && s.contains("acme/c")),
                "expected chain through acme/a → acme/c, got: {rendered:?}"
            );
            assert!(
                rendered.iter().any(|s| s.contains("acme/b") && s.contains("acme/c")),
                "expected chain through acme/b → acme/c, got: {rendered:?}"
            );
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
}
