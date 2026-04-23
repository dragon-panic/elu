mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::thread;
use std::time::Duration;

use camino::Utf8Path;
use elu_outputs::{TarOpts, tar as tar_out};
use tempfile::TempDir;

fn populate(path: &Utf8Path) {
    fs::create_dir_all(path.as_std_path()).unwrap();
    // Use an intentionally "wrong" write order vs. the sorted emission order
    // so sort correctness is actually exercised.
    fs::write(path.join("z.txt").as_std_path(), b"z-content").unwrap();
    fs::write(path.join("a.txt").as_std_path(), b"a-content").unwrap();
    fs::create_dir_all(path.join("m/n").as_std_path()).unwrap();
    fs::write(path.join("m/n/leaf.txt").as_std_path(), b"leaf").unwrap();
    for p in ["z.txt", "a.txt", "m/n/leaf.txt"] {
        fs::set_permissions(
            path.join(p).as_std_path(),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }
}

#[test]
fn two_runs_same_input_produce_identical_bytes() {
    let tmp1 = TempDir::new().unwrap();
    let tmp2 = TempDir::new().unwrap();
    let w1 = Utf8Path::from_path(tmp1.path()).unwrap();
    let w2 = Utf8Path::from_path(tmp2.path()).unwrap();

    let s1 = w1.join("stage");
    let s2 = w2.join("stage");
    populate(&s1);
    // Ensure mtime differences between the two stagings; deterministic mode
    // should erase them from the archive.
    thread::sleep(Duration::from_millis(1100));
    populate(&s2);

    let t1 = w1.join("out.tar");
    let t2 = w2.join("out.tar");

    tar_out::materialize(&s1, &t1, &TarOpts::default()).unwrap();
    tar_out::materialize(&s2, &t2, &TarOpts::default()).unwrap();

    let b1 = fs::read(t1.as_std_path()).unwrap();
    let b2 = fs::read(t2.as_std_path()).unwrap();
    assert_eq!(b1, b2, "tar output not byte-reproducible");
}

#[test]
fn filesystem_order_does_not_leak_into_archive() {
    // Populate in two different orders; sorted emission must produce the
    // same bytes regardless of inode / directory-entry order.
    let tmp1 = TempDir::new().unwrap();
    let tmp2 = TempDir::new().unwrap();
    let w1 = Utf8Path::from_path(tmp1.path()).unwrap();
    let w2 = Utf8Path::from_path(tmp2.path()).unwrap();
    let s1 = w1.join("stage");
    let s2 = w2.join("stage");
    fs::create_dir_all(s1.as_std_path()).unwrap();
    fs::create_dir_all(s2.as_std_path()).unwrap();

    // order A
    fs::write(s1.join("a.txt").as_std_path(), b"A").unwrap();
    fs::write(s1.join("b.txt").as_std_path(), b"B").unwrap();
    fs::write(s1.join("c.txt").as_std_path(), b"C").unwrap();
    // order B
    fs::write(s2.join("c.txt").as_std_path(), b"C").unwrap();
    fs::write(s2.join("a.txt").as_std_path(), b"A").unwrap();
    fs::write(s2.join("b.txt").as_std_path(), b"B").unwrap();
    for s in [&s1, &s2] {
        for p in ["a.txt", "b.txt", "c.txt"] {
            fs::set_permissions(
                s.join(p).as_std_path(),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap();
        }
    }

    let t1 = w1.join("out.tar");
    let t2 = w2.join("out.tar");
    tar_out::materialize(&s1, &t1, &TarOpts::default()).unwrap();
    tar_out::materialize(&s2, &t2, &TarOpts::default()).unwrap();
    assert_eq!(fs::read(t1.as_std_path()).unwrap(), fs::read(t2.as_std_path()).unwrap());
}
