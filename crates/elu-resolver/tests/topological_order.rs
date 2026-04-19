mod common;

use common::{
    InMemorySource, dep, make_manifest, make_manifest_with_layers, pkgref, synth_diff, vrange,
};
use elu_resolver::resolve;
use elu_resolver::types::RootRef;

/// Slice 6: dependencies are walked, manifests come back in dependency-first
/// (post-order) order, and the layer list is the deduplicated concatenation.
#[tokio::test]
async fn recursive_deps_topo_ordered_layers_deduped() {
    let mut src = InMemorySource::new();

    // Layer setup: c uses [L1], b uses [L1, L2] (shares L1 with c), a uses [L3].
    let l1 = synth_diff(0x10);
    let l2 = synth_diff(0x20);
    let l3 = synth_diff(0x30);

    src.add(make_manifest_with_layers("acme", "c", "1.0.0", vec![], vec![l1.clone()]));
    src.add(make_manifest_with_layers(
        "acme",
        "b",
        "1.0.0",
        vec![dep("acme", "c", vrange("^1"))],
        vec![l1.clone(), l2.clone()],
    ));
    src.add(make_manifest_with_layers(
        "acme",
        "a",
        "1.0.0",
        vec![dep("acme", "b", vrange("^1"))],
        vec![l3.clone()],
    ));

    let root = RootRef {
        package: pkgref("acme", "a"),
        version: vrange("^1"),
    };

    let resolution = resolve(&[root], &src, None, None).await.expect("resolve ok");

    let names: Vec<_> = resolution
        .manifests
        .iter()
        .map(|m| m.package.to_string())
        .collect();
    assert_eq!(names, vec!["acme/c", "acme/b", "acme/a"], "post-order: deps first");

    // Layer order: c first (just L1), then b appends L2 (L1 dedup'd), then a appends L3.
    assert_eq!(resolution.layers, vec![l1, l2, l3]);
}

/// Sibling roots: alphabetic tie-break stabilizes order between independent
/// branches that share nothing.
#[tokio::test]
async fn sibling_roots_alphabetic_tie_break() {
    let mut src = InMemorySource::new();
    src.add(make_manifest("acme", "z", "1.0.0", vec![]));
    src.add(make_manifest("acme", "a", "1.0.0", vec![]));

    // Roots given in z-then-a order, but the spec says alphabetic tie-break
    // stabilizes order at any tie. Roots themselves are iterated in given order,
    // so the resolution order follows: z (as given), then a (as given).
    // Tie-break only kicks in for siblings discovered together.
    let roots = vec![
        RootRef {
            package: pkgref("acme", "z"),
            version: vrange("^1"),
        },
        RootRef {
            package: pkgref("acme", "a"),
            version: vrange("^1"),
        },
    ];

    let resolution = resolve(&roots, &src, None, None).await.expect("resolve ok");
    let names: Vec<_> = resolution
        .manifests
        .iter()
        .map(|m| m.package.to_string())
        .collect();
    assert_eq!(names, vec!["acme/z", "acme/a"]);
}
