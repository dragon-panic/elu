use semver::{Version, VersionReq};

/// True if any comparator in `req` mentions a pre-release in its operand.
pub fn req_opts_in_to_prerelease(req: &VersionReq) -> bool {
    req.comparators.iter().any(|c| !c.pre.is_empty())
}

/// Pick the highest version in `candidates` satisfying `req`. Pre-releases
/// are excluded unless the constraint itself mentions a pre-release.
pub fn highest_match<'a>(
    req: &VersionReq,
    candidates: impl IntoIterator<Item = &'a Version>,
) -> Option<Version> {
    let allow_pre = req_opts_in_to_prerelease(req);
    candidates
        .into_iter()
        .filter(|v| (allow_pre || v.pre.is_empty()) && req.matches(v))
        .max()
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(s: &str) -> VersionReq {
        VersionReq::parse(s).unwrap()
    }
    fn ver(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn pick(spec: &str, candidates: &[&str]) -> Option<String> {
        let vs: Vec<Version> = candidates.iter().map(|s| ver(s)).collect();
        highest_match(&req(spec), &vs).map(|v| v.to_string())
    }

    #[test]
    fn caret_picks_highest_compatible() {
        assert_eq!(
            pick("^1.2.0", &["1.0.0", "1.2.5", "1.4.0", "2.0.0"]).as_deref(),
            Some("1.4.0")
        );
    }

    #[test]
    fn tilde_picks_highest_minor_patch() {
        assert_eq!(
            pick("~1.2.3", &["1.2.0", "1.2.9", "1.3.0"]).as_deref(),
            Some("1.2.9")
        );
    }

    #[test]
    fn comparison_combined() {
        assert_eq!(
            pick(">=1.0, <2.0", &["0.9.0", "1.5.0", "2.0.0"]).as_deref(),
            Some("1.5.0")
        );
    }

    #[test]
    fn exact_match_only() {
        assert_eq!(
            pick("=1.2.3", &["1.2.2", "1.2.3", "1.2.4"]).as_deref(),
            Some("1.2.3")
        );
        assert_eq!(pick("=1.2.3", &["1.2.4"]), None);
    }

    #[test]
    fn wildcard_star_matches_anything_stable() {
        assert_eq!(
            pick("*", &["1.0.0", "2.0.0", "3.0.0-alpha"]).as_deref(),
            Some("2.0.0"),
            "pre-releases excluded by default"
        );
    }

    #[test]
    fn intersection_both_must_hold() {
        assert_eq!(
            pick(">=1.0.0, <1.5.0", &["1.4.99", "1.5.0", "1.5.1"]).as_deref(),
            Some("1.4.99")
        );
    }

    #[test]
    fn prerelease_excluded_unless_constraint_opts_in() {
        // Constraint without a pre-release operand → no pre-release matches.
        assert_eq!(pick("^1.0.0", &["1.0.0-rc.1"]), None);
        // Constraint with a pre-release operand → pre-release allowed.
        assert_eq!(
            pick(">=1.0.0-rc.0", &["1.0.0-rc.1"]).as_deref(),
            Some("1.0.0-rc.1")
        );
    }
}
