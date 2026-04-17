use elu_hooks::mode::ModeSpec;

#[test]
fn octal_0755() {
    let m = ModeSpec::parse("0755").unwrap();
    assert_eq!(m.apply(0), 0o755);
}

#[test]
fn octal_0644() {
    let m = ModeSpec::parse("0644").unwrap();
    assert_eq!(m.apply(0), 0o644);
}

#[test]
fn octal_without_leading_zero() {
    let m = ModeSpec::parse("755").unwrap();
    assert_eq!(m.apply(0), 0o755);
}

#[test]
fn octal_replaces_lower_bits() {
    // Type bits (0o100000 = regular file) should be preserved
    let m = ModeSpec::parse("0755").unwrap();
    let result = m.apply(0o100644);
    assert_eq!(result, 0o100755);
}

#[test]
fn symbolic_plus_x() {
    let m = ModeSpec::parse("+x").unwrap();
    // +x with no who defaults to a (all)
    let result = m.apply(0o644);
    assert_eq!(result, 0o755);
}

#[test]
fn symbolic_u_plus_rw() {
    let m = ModeSpec::parse("u+rw").unwrap();
    let result = m.apply(0o000);
    assert_eq!(result, 0o600);
}

#[test]
fn symbolic_g_minus_w() {
    let m = ModeSpec::parse("g-w").unwrap();
    let result = m.apply(0o664);
    assert_eq!(result, 0o644);
}

#[test]
fn symbolic_comma_separated() {
    let m = ModeSpec::parse("u+rw,g-w").unwrap();
    let result = m.apply(0o024);
    assert_eq!(result, 0o604);
}

#[test]
fn symbolic_a_equals_r() {
    let m = ModeSpec::parse("a=r").unwrap();
    let result = m.apply(0o777);
    assert_eq!(result, 0o444);
}

#[test]
fn symbolic_o_minus_rwx() {
    let m = ModeSpec::parse("o-rwx").unwrap();
    let result = m.apply(0o777);
    assert_eq!(result, 0o770);
}

#[test]
fn invalid_mode_rejected() {
    assert!(ModeSpec::parse("").is_err());
    assert!(ModeSpec::parse("hello").is_err());
    assert!(ModeSpec::parse("u+z").is_err());
}
