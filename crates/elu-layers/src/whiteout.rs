pub const WHITEOUT_PREFIX: &str = ".wh.";
pub const OPAQUE_NAME: &str = ".wh..wh..opq";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whiteout<'a> {
    /// Opaque whiteout: clear the parent dir before applying this layer's
    /// entries in it.
    Opaque,
    /// Normal whiteout: delete `<parent>/<name>`.
    Remove(&'a str),
    None,
}

pub fn classify(basename: &str) -> Whiteout<'_> {
    if basename == OPAQUE_NAME {
        Whiteout::Opaque
    } else if let Some(name) = basename.strip_prefix(WHITEOUT_PREFIX) {
        Whiteout::Remove(name)
    } else {
        Whiteout::None
    }
}
