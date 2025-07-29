pub mod discovery;
pub mod watcher;

use ct_core::{compute_file_digest, compute_symbol_id, CoreError};
use ct_core::models::*;
use ct_db::{Database, DbError};
use std::path::PathBuf;
use std::collections::HashMap;
use thiserror::Error;
use tracing::info;
use serde::{Deserialize, Serialize};

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("Database error: {0}")]
    Database(#[from] DbError),
    
    #[error("Core error: {0}")]
    Core(#[from] CoreError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),
    
    #[error("Indexing failed: {0}")]
    IndexingFailed(String),
}

pub type Result<T> = std::result::Result<T, IndexError>;

pub struct Indexer {
    workspace_root: PathBuf,
    db: Database,
    crate_cache: HashMap<String, i64>,
    _file_cache: HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMember {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub package_id: String,
}

impl Indexer {
    pub fn new(workspace_root: PathBuf, db: Database) -> Self {
        Self {
            workspace_root,
            db,
            crate_cache: HashMap::new(),
            _file_cache: HashMap::new(),
        }
    }

    pub async fn index_workspace(&mut self) -> Result<IndexStats> {
        info!("Starting workspace indexing at {:?}", self.workspace_root);
        
        let start = std::time::Instant::now();
        let members = discovery::discover_workspace_members(&self.workspace_root).await?;
        
        info!("Found {} workspace members", members.len());
        
        self.db.begin_transaction()?;
        
        let mut stats = IndexStats::default();
        
        for member in &members {
            info!("Indexing crate: {} ({})", member.name, member.version);
            let crate_stats = self.index_crate(member).await?;
            stats.merge(crate_stats);
        }
        
        self.db.commit_transaction()?;
        
        stats.duration_ms = start.elapsed().as_millis() as u64;
        info!("Indexing completed in {}ms", stats.duration_ms);
        
        Ok(stats)
    }

    async fn index_crate(&mut self, member: &WorkspaceMember) -> Result<IndexStats> {
        let mut stats = IndexStats::default();
        
        // Create crate entry
        let crate_fingerprint = self.compute_crate_fingerprint(member)?;
        let crate_id = self.db.insert_crate(
            &member.name,
            Some(&member.version),
            &crate_fingerprint,
        )?;
        
        self.crate_cache.insert(member.name.clone(), crate_id);
        stats.crates_indexed += 1;
        
        // Stub: In a real implementation, we would run rustdoc --output-format json here
        // and parse the output. For now, create some stub entries.
        
        // Create a stub file
        let src_main = member.path.join("src/lib.rs");
        if src_main.exists() {
            let content = std::fs::read(&src_main)?;
            let digest = compute_file_digest(&content);
            
            let file_id = self.db.insert_file(
                crate_id,
                &src_main.to_string_lossy(),
                &digest,
            )?;
            
            stats.files_indexed += 1;
            
            // Create stub symbols
            let stub_symbols = vec![
                Symbol {
                    symbol_id: compute_symbol_id(
                        &format!("{}::lib", member.name),
                        "module",
                        &digest,
                        1,
                        100,
                    ),
                    crate_id,
                    file_id,
                    path: format!("{}::lib", member.name),
                    name: "lib".to_string(),
                    kind: SymbolKind::Module,
                    visibility: Visibility::Public,
                    signature: format!("pub mod lib"),
                    docs: Some("Stub module documentation".to_string()),
                    status: ImplementationStatus::Implemented,
                    span_start: 1,
                    span_end: 100,
                    def_hash: "stub_hash".to_string(),
                },
            ];
            
            for symbol in stub_symbols {
                self.db.insert_symbol(&symbol)?;
                stats.symbols_indexed += 1;
            }
        }
        
        Ok(stats)
    }

    fn compute_crate_fingerprint(&self, member: &WorkspaceMember) -> Result<String> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(member.name.as_bytes());
        hasher.update(member.version.as_bytes());
        hasher.update(member.package_id.as_bytes());
        
        // In real implementation, would include:
        // - rustc version hash
        // - features
        // - target
        // - cfg snapshot
        
        Ok(format!("blake3:{}", hasher.finalize().to_hex()))
    }

    pub async fn reindex_files(&mut self, changed_files: Vec<PathBuf>) -> Result<IndexStats> {
        info!("Reindexing {} changed files", changed_files.len());
        
        // Stub: In real implementation, would:
        // 1. Determine which crates are affected
        // 2. Re-run rustdoc for those crates only
        // 3. Update the database incrementally
        
        Ok(IndexStats::default())
    }
}

#[derive(Debug, Default)]
pub struct IndexStats {
    pub crates_indexed: usize,
    pub files_indexed: usize,
    pub symbols_indexed: usize,
    pub duration_ms: u64,
}

impl IndexStats {
    fn merge(&mut self, other: IndexStats) {
        self.crates_indexed += other.crates_indexed;
        self.files_indexed += other.files_indexed;
        self.symbols_indexed += other.symbols_indexed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_indexer_creation() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open_temp(temp_dir.path().join("test.db").as_path())
            .map_err(IndexError::Database)?;
        
        let indexer = Indexer::new(temp_dir.path().to_path_buf(), db);
        assert_eq!(indexer.crate_cache.len(), 0);
        
        Ok(())
    }
}