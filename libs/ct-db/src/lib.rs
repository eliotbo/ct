pub mod migrations;
pub mod queries;

use ct_core::models::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    
    #[error("Migration error: {0}")]
    Migration(String),
    
    #[error("Schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: String, found: String },
}

pub type Result<T> = std::result::Result<T, DbError>;

pub struct Database {
    pub(crate) conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        
        // Enable WAL mode and set pragmas
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "mmap_size", 30000000)?;
        conn.pragma_update(None, "page_size", 4096)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        
        let mut db = Self { conn };
        db.ensure_schema()?;
        Ok(db)
    }

    pub fn open_temp(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        
        // Same pragmas for temp DB
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        
        let mut db = Self { conn };
        db.ensure_schema()?;
        Ok(db)
    }

    fn ensure_schema(&mut self) -> Result<()> {
        let version = self.get_schema_version()?;
        
        if version == 0 {
            info!("Creating initial schema");
            self.apply_migration(&migrations::V1_SCHEMA)?;
            self.set_schema_version(1)?;
        } else if version < migrations::CURRENT_VERSION {
            return Err(DbError::SchemaMismatch {
                expected: migrations::CURRENT_VERSION.to_string(),
                found: version.to_string(),
            });
        }
        
        Ok(())
    }

    fn get_schema_version(&self) -> Result<u32> {
        // Check if meta table exists first
        let table_exists: bool = self.conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='meta'",
                [],
                |row| row.get::<_, i64>(0).map(|count| count > 0),
            )?;
            
        if !table_exists {
            return Ok(0);
        }
        
        let version: Option<String> = self.conn
            .query_row(
                "SELECT val FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
            
        Ok(version.and_then(|v| v.parse().ok()).unwrap_or(0))
    }

    fn set_schema_version(&self, version: u32) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, val) VALUES ('schema_version', ?)",
            params![version.to_string()],
        )?;
        Ok(())
    }

    fn apply_migration(&self, migration: &str) -> Result<()> {
        self.conn.execute_batch(migration)?;
        Ok(())
    }

    pub fn insert_crate(&self, name: &str, version: Option<&str>, fingerprint: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO crates (name, version, fingerprint) VALUES (?, ?, ?)",
            params![name, version, fingerprint],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_file(&self, crate_id: i64, path: &str, digest: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO files (crate_id, path, digest) VALUES (?, ?, ?)",
            params![crate_id, path, digest],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_symbol(&self, symbol: &Symbol) -> Result<()> {
        self.conn.execute(
            "INSERT INTO symbols (
                symbol_id, crate_id, file_id, path, name, kind, visibility,
                signature, docs, status, span_start, span_end, def_hash
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                &symbol.symbol_id.as_bytes()[..],
                symbol.crate_id,
                symbol.file_id,
                &symbol.path,
                &symbol.name,
                symbol.kind.as_str(),
                symbol.visibility.as_str(),
                &symbol.signature,
                &symbol.docs,
                symbol.status.as_str(),
                symbol.span_start,
                symbol.span_end,
                &symbol.def_hash,
            ],
        )?;
        Ok(())
    }

    pub fn insert_impl(&self, imp: &ImplBlock) -> Result<()> {
        self.conn.execute(
            "INSERT INTO impls (for_path, trait_path, file_id, line_start, line_end)
             VALUES (?, ?, ?, ?, ?)",
            params![
                &imp.for_path,
                &imp.trait_path,
                imp.file_id,
                imp.line_start,
                imp.line_end,
            ],
        )?;
        Ok(())
    }

    pub fn insert_reference(&self, reference: &Reference) -> Result<()> {
        self.conn.execute(
            "INSERT INTO symbol_references (symbol_id, target_path, file_id, span_start, span_end)
             VALUES (?, ?, ?, ?, ?)",
            params![
                reference.symbol_id,
                &reference.target_path,
                reference.file_id,
                reference.span_start,
                reference.span_end,
            ],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let val: Option<String> = self.conn
            .query_row(
                "SELECT val FROM meta WHERE key = ?",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(val)
    }

    pub fn set_meta(&self, key: &str, val: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, val) VALUES (?, ?)",
            params![key, val],
        )?;
        Ok(())
    }

    pub fn get_symbol_count(&self) -> Result<usize> {
        let count: usize = self.conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn get_crate_count(&self) -> Result<usize> {
        let count: usize = self.conn
            .query_row("SELECT COUNT(*) FROM crates", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn get_file_count(&self) -> Result<usize> {
        let count: usize = self.conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn begin_transaction(&mut self) -> Result<()> {
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        Ok(())
    }

    pub fn commit_transaction(&mut self) -> Result<()> {
        self.conn.execute("COMMIT", [])?;
        Ok(())
    }

    pub fn rollback_transaction(&mut self) -> Result<()> {
        self.conn.execute("ROLLBACK", [])?;
        Ok(())
    }
    
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_database_creation() -> Result<()> {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path())?;
        
        assert_eq!(db.get_schema_version()?, 1);
        assert_eq!(db.get_symbol_count()?, 0);
        
        Ok(())
    }

    #[test]
    fn test_insert_crate() -> Result<()> {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open(temp.path())?;
        
        let crate_id = db.insert_crate("test_crate", Some("0.1.0"), "fingerprint123")?;
        assert_eq!(crate_id, 1);
        assert_eq!(db.get_crate_count()?, 1);
        
        Ok(())
    }
}