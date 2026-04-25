use std::sync::Mutex;

use elu_store::hash::{BlobId, DiffId, ManifestHash};
use rusqlite::{Connection, params};
use url::Url;

use crate::error::RegistryError;
use crate::types::*;

pub struct SqliteRegistryDb {
    conn: Mutex<Connection>,
}

impl SqliteRegistryDb {
    pub fn open_in_memory() -> Result<Self, RegistryError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), RegistryError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS namespaces (
                namespace TEXT PRIMARY KEY,
                owner TEXT NOT NULL,
                verified INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS package_versions (
                namespace TEXT NOT NULL,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                manifest_blob_id TEXT NOT NULL,
                manifest_url TEXT NOT NULL,
                kind TEXT,
                description TEXT,
                tags TEXT,
                publisher TEXT NOT NULL,
                published_at TEXT NOT NULL,
                signature TEXT,
                visibility TEXT NOT NULL DEFAULT 'public',
                PRIMARY KEY (namespace, name, version)
            );

            CREATE TABLE IF NOT EXISTS layers (
                namespace TEXT NOT NULL,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                diff_id TEXT NOT NULL,
                blob_id TEXT NOT NULL,
                url TEXT NOT NULL,
                size_compressed INTEGER NOT NULL,
                size_uncompressed INTEGER NOT NULL,
                FOREIGN KEY (namespace, name, version)
                    REFERENCES package_versions(namespace, name, version)
            );

            CREATE TABLE IF NOT EXISTS publish_sessions (
                session_id TEXT PRIMARY KEY,
                namespace TEXT NOT NULL,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                manifest_blob_id TEXT NOT NULL,
                manifest_bytes BLOB NOT NULL,
                layers_json TEXT NOT NULL,
                publisher TEXT NOT NULL,
                visibility TEXT NOT NULL DEFAULT 'public',
                created_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    /// Insert a committed package version with its layers.
    pub fn put_version(&self, record: &PackageRecord) -> Result<(), RegistryError> {
        let conn = self.conn.lock().unwrap();
        let tags_json = serde_json::to_string(&record.tags)
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        let vis = match record.visibility {
            Visibility::Public => "public",
            Visibility::Private => "private",
        };

        conn.execute(
            "INSERT INTO package_versions
             (namespace, name, version, manifest_blob_id, manifest_url,
              kind, description, tags, publisher, published_at, signature, visibility)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.namespace,
                record.name,
                record.version,
                record.manifest_blob_id.to_string(),
                record.manifest_url.as_str(),
                record.kind,
                record.description,
                tags_json,
                record.publisher,
                record.published_at,
                record.signature,
                vis,
            ],
        )
        .map_err(|e| {
            if let rusqlite::Error::SqliteFailure(ref err, _) = e
                && err.code == rusqlite::ErrorCode::ConstraintViolation {
                    return RegistryError::VersionExists {
                        namespace: record.namespace.clone(),
                        name: record.name.clone(),
                        version: record.version.clone(),
                    };
                }
            RegistryError::from(e)
        })?;

        for layer in &record.layers {
            conn.execute(
                "INSERT INTO layers
                 (namespace, name, version, diff_id, blob_id, url, size_compressed, size_uncompressed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    record.namespace,
                    record.name,
                    record.version,
                    layer.diff_id.to_string(),
                    layer.blob_id.to_string(),
                    layer.url.as_str(),
                    layer.size_compressed,
                    layer.size_uncompressed,
                ],
            )?;
        }

        Ok(())
    }

    /// Retrieve a package version by namespace/name/version.
    pub fn get_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<PackageRecord, RegistryError> {
        let conn = self.conn.lock().unwrap();

        let row = conn
            .query_row(
                "SELECT manifest_blob_id, manifest_url, kind, description, tags,
                        publisher, published_at, signature, visibility
                 FROM package_versions
                 WHERE namespace = ?1 AND name = ?2 AND version = ?3",
                params![namespace, name, version],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => RegistryError::VersionNotFound {
                    namespace: namespace.to_string(),
                    name: name.to_string(),
                    version: version.to_string(),
                },
                other => RegistryError::from(other),
            })?;

        let (
            manifest_blob_id_str,
            manifest_url_str,
            kind,
            description,
            tags_json,
            publisher,
            published_at,
            signature,
            visibility_str,
        ) = row;

        let manifest_blob_id: ManifestHash = manifest_blob_id_str
            .parse()
            .map_err(|e| RegistryError::Database(format!("bad manifest hash: {e}")))?;
        let manifest_url = Url::parse(&manifest_url_str)
            .map_err(|e| RegistryError::Database(format!("bad manifest url: {e}")))?;
        let tags: Vec<String> = serde_json::from_str(&tags_json)
            .map_err(|e| RegistryError::Database(format!("bad tags json: {e}")))?;
        let visibility = match visibility_str.as_str() {
            "private" => Visibility::Private,
            _ => Visibility::Public,
        };

        // Fetch layers
        let mut stmt = conn.prepare(
            "SELECT diff_id, blob_id, url, size_compressed, size_uncompressed
             FROM layers
             WHERE namespace = ?1 AND name = ?2 AND version = ?3",
        )?;

        let layers = stmt
            .query_map(params![namespace, name, version], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, u64>(4)?,
                ))
            })?
            .map(|r| {
                let (diff_id_str, blob_id_str, url_str, sc, su) = r?;
                let diff_id: DiffId = diff_id_str
                    .parse()
                    .map_err(|e| RegistryError::Database(format!("bad diff_id: {e}")))?;
                let blob_id: BlobId = blob_id_str
                    .parse()
                    .map_err(|e| RegistryError::Database(format!("bad blob_id: {e}")))?;
                let url = Url::parse(&url_str)
                    .map_err(|e| RegistryError::Database(format!("bad layer url: {e}")))?;
                Ok(LayerRecord {
                    diff_id,
                    blob_id,
                    url,
                    size_compressed: sc,
                    size_uncompressed: su,
                })
            })
            .collect::<Result<Vec<_>, RegistryError>>()?;

        Ok(PackageRecord {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            manifest_blob_id,
            manifest_url,
            kind,
            description,
            tags,
            layers,
            publisher,
            published_at,
            signature,
            visibility,
        })
    }

    /// List versions for a given namespace/name, newest first (by published_at).
    pub fn list_versions(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<Vec<VersionEntry>, RegistryError> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT version, published_at, kind
             FROM package_versions
             WHERE namespace = ?1 AND name = ?2
             ORDER BY published_at DESC",
        )?;

        let entries = stmt
            .query_map(params![namespace, name], |row| {
                Ok(VersionEntry {
                    version: row.get(0)?,
                    published_at: row.get(1)?,
                    kind: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if entries.is_empty() {
            return Err(RegistryError::PackageNotFound {
                namespace: namespace.to_string(),
                name: name.to_string(),
            });
        }

        Ok(entries)
    }

    /// Create a pending publish session.
    #[allow(clippy::too_many_arguments)]
    pub fn put_publish_session(
        &self,
        session_id: &str,
        namespace: &str,
        name: &str,
        version: &str,
        manifest_blob_id: &ManifestHash,
        manifest_bytes: &[u8],
        layers: &[PublishLayerRecord],
        publisher: &str,
        visibility: Visibility,
        created_at: &str,
    ) -> Result<(), RegistryError> {
        let conn = self.conn.lock().unwrap();

        // Check if version already exists
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM package_versions
                 WHERE namespace = ?1 AND name = ?2 AND version = ?3",
                params![namespace, name, version],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)?;

        if exists {
            return Err(RegistryError::VersionExists {
                namespace: namespace.to_string(),
                name: name.to_string(),
                version: version.to_string(),
            });
        }

        let layers_json = serde_json::to_string(layers)
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        let vis = match visibility {
            Visibility::Public => "public",
            Visibility::Private => "private",
        };

        conn.execute(
            "INSERT INTO publish_sessions
             (session_id, namespace, name, version, manifest_blob_id,
              manifest_bytes, layers_json, publisher, visibility, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                session_id,
                namespace,
                name,
                version,
                manifest_blob_id.to_string(),
                manifest_bytes,
                layers_json,
                publisher,
                vis,
                created_at,
            ],
        )?;

        Ok(())
    }

    /// Retrieve a pending publish session.
    pub fn get_publish_session(
        &self,
        session_id: &str,
    ) -> Result<PublishSession, RegistryError> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT session_id, namespace, name, version, manifest_blob_id,
                    manifest_bytes, layers_json, publisher, visibility, created_at
             FROM publish_sessions
             WHERE session_id = ?1",
            params![session_id],
            |row| {
                Ok(PublishSession {
                    session_id: row.get(0)?,
                    namespace: row.get(1)?,
                    name: row.get(2)?,
                    version: row.get(3)?,
                    manifest_blob_id: row.get(4)?,
                    manifest_bytes: row.get(5)?,
                    layers_json: row.get(6)?,
                    publisher: row.get(7)?,
                    visibility: row.get(8)?,
                    created_at: row.get(9)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => RegistryError::SessionNotFound {
                session_id: session_id.to_string(),
            },
            other => RegistryError::from(other),
        })
    }

    /// Commit a publish session: insert into package_versions and delete the session.
    pub fn commit_version(
        &self,
        session_id: &str,
        manifest_url: &Url,
        layer_urls: &[(BlobId, Url)],
    ) -> Result<PackageRecord, RegistryError> {
        let conn = self.conn.lock().unwrap();

        let session: PublishSession = conn
            .query_row(
                "SELECT session_id, namespace, name, version, manifest_blob_id,
                        manifest_bytes, layers_json, publisher, visibility, created_at
                 FROM publish_sessions WHERE session_id = ?1",
                params![session_id],
                |row| {
                    Ok(PublishSession {
                        session_id: row.get(0)?,
                        namespace: row.get(1)?,
                        name: row.get(2)?,
                        version: row.get(3)?,
                        manifest_blob_id: row.get(4)?,
                        manifest_bytes: row.get(5)?,
                        layers_json: row.get(6)?,
                        publisher: row.get(7)?,
                        visibility: row.get(8)?,
                        created_at: row.get(9)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => RegistryError::SessionNotFound {
                    session_id: session_id.to_string(),
                },
                other => RegistryError::from(other),
            })?;

        let manifest_blob_id: ManifestHash = session
            .manifest_blob_id
            .parse()
            .map_err(|e| RegistryError::Database(format!("bad manifest hash: {e}")))?;

        let publish_layers: Vec<PublishLayerRecord> =
            serde_json::from_str(&session.layers_json)
                .map_err(|e| RegistryError::Database(format!("bad layers json: {e}")))?;

        let visibility = match session.visibility.as_str() {
            "private" => Visibility::Private,
            _ => Visibility::Public,
        };

        let vis_str = &session.visibility;
        let tags_json = "[]";

        // Check version doesn't already exist (race condition guard)
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM package_versions
                 WHERE namespace = ?1 AND name = ?2 AND version = ?3",
                params![session.namespace, session.name, session.version],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)?;

        if exists {
            return Err(RegistryError::VersionExists {
                namespace: session.namespace.clone(),
                name: session.name.clone(),
                version: session.version.clone(),
            });
        }

        conn.execute(
            "INSERT INTO package_versions
             (namespace, name, version, manifest_blob_id, manifest_url,
              kind, description, tags, publisher, published_at, signature, visibility)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                session.namespace,
                session.name,
                session.version,
                session.manifest_blob_id,
                manifest_url.as_str(),
                Option::<String>::None,
                Option::<String>::None,
                tags_json,
                session.publisher,
                session.created_at,
                Option::<String>::None,
                vis_str,
            ],
        )?;

        // Build URL map for layers
        let url_map: std::collections::HashMap<String, &Url> = layer_urls
            .iter()
            .map(|(bid, url)| (bid.to_string(), url))
            .collect();

        let mut layers = Vec::new();
        for pl in &publish_layers {
            let blob_key = pl.blob_id.to_string();
            let url = url_map
                .get(&blob_key)
                .ok_or_else(|| RegistryError::MissingBlobs {
                    blob_ids: vec![blob_key.clone()],
                })?;

            conn.execute(
                "INSERT INTO layers
                 (namespace, name, version, diff_id, blob_id, url, size_compressed, size_uncompressed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    session.namespace,
                    session.name,
                    session.version,
                    pl.diff_id.to_string(),
                    pl.blob_id.to_string(),
                    url.as_str(),
                    pl.size_compressed,
                    pl.size_uncompressed,
                ],
            )?;

            layers.push(LayerRecord {
                diff_id: pl.diff_id.clone(),
                blob_id: pl.blob_id.clone(),
                url: (*url).clone(),
                size_compressed: pl.size_compressed,
                size_uncompressed: pl.size_uncompressed,
            });
        }

        // Delete session
        conn.execute(
            "DELETE FROM publish_sessions WHERE session_id = ?1",
            params![session_id],
        )?;

        Ok(PackageRecord {
            namespace: session.namespace,
            name: session.name,
            version: session.version,
            manifest_blob_id,
            manifest_url: manifest_url.clone(),
            kind: None,
            description: None,
            tags: vec![],
            layers,
            publisher: session.publisher,
            published_at: session.created_at,
            signature: None,
            visibility,
        })
    }

    /// Search packages using text matching on name/description/tags with optional filters.
    pub fn search(
        &self,
        query: &SearchQuery,
        authenticated_ns: Option<&str>,
    ) -> Result<Vec<SearchResult>, RegistryError> {
        let conn = self.conn.lock().unwrap();

        // Build the query dynamically based on filters
        let mut sql = String::from(
            "SELECT namespace, name, version, kind, description, tags, published_at, visibility
             FROM package_versions
             WHERE 1=1",
        );
        let mut bind_values: Vec<String> = Vec::new();

        if let Some(ref q) = query.q {
            bind_values.push(format!("%{q}%"));
            sql.push_str(&format!(
                " AND (name LIKE ?{i} OR description LIKE ?{i})",
                i = bind_values.len()
            ));
        }
        if let Some(ref kind) = query.kind {
            bind_values.push(kind.clone());
            sql.push_str(&format!(" AND kind = ?{}", bind_values.len()));
        }
        if let Some(ref tag) = query.tag {
            bind_values.push(format!("%\"{tag}\"%"));
            sql.push_str(&format!(" AND tags LIKE ?{}", bind_values.len()));
        }
        if let Some(ref ns) = query.namespace {
            bind_values.push(ns.clone());
            sql.push_str(&format!(" AND namespace = ?{}", bind_values.len()));
        }

        // Only show latest version per package, and filter visibility
        sql.push_str(
            " AND (namespace, name, published_at) IN (
                SELECT namespace, name, MAX(published_at)
                FROM package_versions
                GROUP BY namespace, name
             )
             ORDER BY published_at DESC",
        );

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            bind_values.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

        let results = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?
            .filter_map(|r| {
                let (ns, name, version, kind, description, tags_json, published_at, vis) =
                    r.ok()?;
                // Filter private packages
                if vis == "private" {
                    if let Some(auth_ns) = authenticated_ns {
                        if auth_ns != ns {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Some(SearchResult {
                    namespace: ns,
                    name,
                    version,
                    kind,
                    description,
                    tags,
                    published_at,
                })
            })
            .collect();

        Ok(results)
    }

    /// Look up a committed package version by its manifest hash. Returns the
    /// same `PackageRecord` shape as `get_version`. Errors with
    /// `ManifestHashNotFound` if no row references that hash.
    pub fn get_version_by_manifest_hash(
        &self,
        hash: &ManifestHash,
    ) -> Result<PackageRecord, RegistryError> {
        unimplemented!("get_version_by_manifest_hash — implemented in green slice")
    }

    /// Hash-keyed variant of `get_version_with_visibility`. Hides private
    /// packages from unauthenticated callers (returns `ManifestHashNotFound`,
    /// not 403, to avoid leaking existence).
    pub fn get_version_by_manifest_hash_with_visibility(
        &self,
        hash: &ManifestHash,
        authenticated_ns: Option<&str>,
    ) -> Result<PackageRecord, RegistryError> {
        unimplemented!(
            "get_version_by_manifest_hash_with_visibility — implemented in green slice"
        )
    }

    /// Get version with visibility enforcement.
    /// Private packages are only returned if authenticated_ns matches the package namespace.
    pub fn get_version_with_visibility(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        authenticated_ns: Option<&str>,
    ) -> Result<PackageRecord, RegistryError> {
        let record = self.get_version(namespace, name, version)?;
        if record.visibility == Visibility::Private {
            match authenticated_ns {
                Some(ns) if ns == namespace => Ok(record),
                _ => Err(RegistryError::VersionNotFound {
                    namespace: namespace.to_string(),
                    name: name.to_string(),
                    version: version.to_string(),
                }),
            }
        } else {
            Ok(record)
        }
    }

    /// List versions with visibility filtering.
    pub fn list_versions_with_visibility(
        &self,
        namespace: &str,
        name: &str,
        authenticated_ns: Option<&str>,
    ) -> Result<Vec<VersionEntry>, RegistryError> {
        // Check if any version is private — if so, require auth
        let conn = self.conn.lock().unwrap();
        let has_private: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM package_versions
                 WHERE namespace = ?1 AND name = ?2 AND visibility = 'private'",
                params![namespace, name],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)?;
        drop(conn);

        if has_private {
            match authenticated_ns {
                Some(ns) if ns == namespace => {}
                _ => {
                    // For private packages, return not found to unauthenticated
                    return Err(RegistryError::PackageNotFound {
                        namespace: namespace.to_string(),
                        name: name.to_string(),
                    });
                }
            }
        }

        self.list_versions(namespace, name)
    }

    /// Put namespace info.
    pub fn put_namespace(&self, info: &NamespaceInfo) -> Result<(), RegistryError> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO namespaces (namespace, owner, verified, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                info.namespace,
                info.owner,
                info.verified as i32,
                info.created_at,
            ],
        )
        .map_err(|e| {
            if let rusqlite::Error::SqliteFailure(ref err, _) = e
                && err.code == rusqlite::ErrorCode::ConstraintViolation {
                    return RegistryError::NamespaceAlreadyClaimed {
                        namespace: info.namespace.clone(),
                    };
                }
            RegistryError::from(e)
        })?;

        Ok(())
    }

    /// Get namespace info.
    pub fn get_namespace(&self, namespace: &str) -> Result<NamespaceInfo, RegistryError> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT namespace, owner, verified, created_at FROM namespaces WHERE namespace = ?1",
            params![namespace],
            |row| {
                Ok(NamespaceInfo {
                    namespace: row.get(0)?,
                    owner: row.get(1)?,
                    verified: row.get::<_, i32>(2)? != 0,
                    created_at: row.get(3)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => RegistryError::NamespaceNotFound {
                namespace: namespace.to_string(),
            },
            other => RegistryError::from(other),
        })
    }

    /// Find a publish session by namespace/name/version.
    pub fn find_session_by_package(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<PublishSession, RegistryError> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT session_id, namespace, name, version, manifest_blob_id,
                    manifest_bytes, layers_json, publisher, visibility, created_at
             FROM publish_sessions
             WHERE namespace = ?1 AND name = ?2 AND version = ?3",
            params![namespace, name, version],
            |row| {
                Ok(PublishSession {
                    session_id: row.get(0)?,
                    namespace: row.get(1)?,
                    name: row.get(2)?,
                    version: row.get(3)?,
                    manifest_blob_id: row.get(4)?,
                    manifest_bytes: row.get(5)?,
                    layers_json: row.get(6)?,
                    publisher: row.get(7)?,
                    visibility: row.get(8)?,
                    created_at: row.get(9)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => RegistryError::SessionNotFound {
                session_id: format!("{namespace}/{name}@{version}"),
            },
            other => RegistryError::from(other),
        })
    }
}

/// Raw publish session data from the database.
pub struct PublishSession {
    pub session_id: String,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub manifest_blob_id: String,
    pub manifest_bytes: Vec<u8>,
    pub layers_json: String,
    pub publisher: String,
    pub visibility: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use elu_store::hash::{Hash, HashAlgo};

    fn h(byte: u8) -> ManifestHash {
        ManifestHash(Hash::new(HashAlgo::Sha256, [byte; 32]))
    }

    fn blob_h(byte: u8) -> BlobId {
        BlobId(Hash::new(HashAlgo::Sha256, [byte; 32]))
    }

    fn diff_h(byte: u8) -> DiffId {
        DiffId(Hash::new(HashAlgo::Sha256, [byte; 32]))
    }

    fn record(ns: &str, name: &str, version: &str, manifest_hash: ManifestHash, vis: Visibility) -> PackageRecord {
        PackageRecord {
            namespace: ns.into(),
            name: name.into(),
            version: version.into(),
            manifest_blob_id: manifest_hash,
            manifest_url: Url::parse("http://example.test/m").unwrap(),
            kind: Some("native".into()),
            description: Some("test".into()),
            tags: vec![],
            layers: vec![LayerRecord {
                diff_id: diff_h(0xaa),
                blob_id: blob_h(0xbb),
                url: Url::parse("http://example.test/l").unwrap(),
                size_compressed: 1,
                size_uncompressed: 2,
            }],
            publisher: "alice".into(),
            published_at: "2026-04-25T00:00:00Z".into(),
            signature: None,
            visibility: vis,
        }
    }

    #[test]
    fn get_version_by_manifest_hash_returns_same_record_as_named_lookup() {
        let db = SqliteRegistryDb::open_in_memory().unwrap();
        let hash = h(0x11);
        let r = record("ns", "demo", "0.1.0", hash.clone(), Visibility::Public);
        db.put_version(&r).unwrap();

        let by_named = db.get_version("ns", "demo", "0.1.0").unwrap();
        let by_hash = db.get_version_by_manifest_hash(&hash).unwrap();

        assert_eq!(by_named, by_hash);
    }

    #[test]
    fn get_version_by_manifest_hash_unknown_hash_errors() {
        let db = SqliteRegistryDb::open_in_memory().unwrap();
        let err = db.get_version_by_manifest_hash(&h(0x99)).unwrap_err();
        assert!(
            matches!(err, RegistryError::ManifestHashNotFound { .. }),
            "expected ManifestHashNotFound, got: {err:?}"
        );
    }

    #[test]
    fn visibility_hides_private_from_unauthenticated() {
        let db = SqliteRegistryDb::open_in_memory().unwrap();
        let hash = h(0x22);
        let r = record("acme", "secret", "0.1.0", hash.clone(), Visibility::Private);
        db.put_version(&r).unwrap();

        let err = db
            .get_version_by_manifest_hash_with_visibility(&hash, None)
            .unwrap_err();
        assert!(
            matches!(err, RegistryError::ManifestHashNotFound { .. }),
            "private package must look like 'not found' to anonymous, got: {err:?}"
        );

        let ok = db
            .get_version_by_manifest_hash_with_visibility(&hash, Some("acme"))
            .unwrap();
        assert_eq!(ok.namespace, "acme");
    }
}
