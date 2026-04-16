use sha2::{Digest, Sha256};

use crate::hash::{Hash, HashAlgo};

pub struct Hasher {
    inner: Sha256,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            inner: Sha256::new(),
        }
    }

    pub fn update(&mut self, chunk: &[u8]) {
        self.inner.update(chunk);
    }

    pub fn finalize(self) -> Hash {
        let out: [u8; 32] = self.inner.finalize().into();
        Hash::new(HashAlgo::Sha256, out)
    }
}
