use std::io::Write;
use std::sync::Arc;

use elu_store::atomic::FsyncMode;
use elu_store::fs_store::FsStore;
use elu_store::store::Store;

fn make_tar(filename: &str, content: &[u8]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_path(filename).unwrap();
    header.set_size(content.len() as u64);
    header.set_cksum();
    builder.append(&header, content).unwrap();
    builder.into_inner().unwrap()
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn two_concurrent_writers_converge() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = camino::Utf8Path::from_path(dir.path()).unwrap().to_owned();
    let store = FsStore::init_with_fsync(&root, FsyncMode::Never).unwrap();

    let tar_bytes = make_tar("concurrent.txt", b"identical content for both writers");
    let gz_bytes = Arc::new(gzip(&tar_bytes));

    let store_root = root.clone();
    let gz1 = Arc::clone(&gz_bytes);
    let gz2 = Arc::clone(&gz_bytes);

    let h1 = std::thread::spawn(move || {
        let s = FsStore::init_with_fsync(&store_root, FsyncMode::Never).unwrap();
        s.put_blob(&mut &gz1[..]).unwrap()
    });

    let h2 = std::thread::spawn(move || {
        store.put_blob(&mut &gz2[..]).unwrap()
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    // Both produce the same blob_id and diff_id
    assert_eq!(r1.blob_id, r2.blob_id);
    assert_eq!(r1.diff_id, r2.diff_id);

    // The object exists exactly once on disk
    let reopened = FsStore::open(&root).unwrap();
    assert!(reopened.has(&r1.blob_id).unwrap());

    // Verify the data is correct
    let data = reopened.get(&r1.blob_id).unwrap().unwrap();
    assert_eq!(&data[..], &gz_bytes[..]);
}
