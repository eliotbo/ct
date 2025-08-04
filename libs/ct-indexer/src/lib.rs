pub mod discovery;
pub mod watcher;

use ct_core::models::{ImplBlock, ImplementationStatus, Symbol, SymbolKind, Visibility};
use ct_core::{compute_file_digest, compute_symbol_id, CoreError};
use ct_db::{Database, DbError};
use rustdoc_types::{Crate, Id, Item, ItemEnum, Type};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use tracing::{error, info, warn};

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

// Common derive trait methods to filter out
const DERIVE_METHODS: &[&str] = &[
    "clone",
    "clone_from",
    "fmt",
    "eq",
    "ne",
    "partial_cmp",
    "cmp",
    "hash",
    "serialize",
    "deserialize",
    "default",
    "from",
    "into",
    "try_from",
    "try_into",
    "as_ref",
    "as_mut",
    "borrow",
    "borrow_mut",
    "to_owned",
    "to_string",
    "drop",
    "deref",
    "deref_mut",
];

fn is_derive_method(method_name: &str) -> bool {
    DERIVE_METHODS.contains(&method_name)
}

pub struct Indexer {
    workspace_root: PathBuf,
    db: Database,
    crate_cache: HashMap<String, i64>,
    file_cache: HashMap<String, i64>,
    filter_module: Option<String>,
    filter_struct: Option<String>,
    include_derives: bool,
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
            file_cache: HashMap::new(),
            filter_module: None,
            filter_struct: None,
            include_derives: false,
        }
    }

    pub fn with_filters(
        mut self,
        module: Option<String>,
        struct_name: Option<String>,
        include_derives: bool,
    ) -> Self {
        self.filter_module = module;
        self.filter_struct = struct_name;
        self.include_derives = include_derives;
        self
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
        let crate_id =
            self.db
                .insert_crate(&member.name, Some(&member.version), &crate_fingerprint)?;

        self.crate_cache.insert(member.name.clone(), crate_id);
        stats.crates_indexed += 1;

        // Generate rustdoc JSON
        let rustdoc_json = self.generate_rustdoc_json(member).await?;

        // Parse the rustdoc JSON
        match self.parse_rustdoc_json(&rustdoc_json) {
            Ok(krate) => {
                info!(
                    "Parsed rustdoc JSON for {}: {} items in index",
                    member.name,
                    krate.index.len()
                );
                // Process the parsed rustdoc data
                self.process_rustdoc_data(&krate, crate_id, &member.name, &mut stats)?;
            }
            Err(e) => {
                error!(
                    "Failed to parse rustdoc JSON for crate {}: {}",
                    member.name, e
                );
                return Err(e);
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

    async fn generate_rustdoc_json(&self, member: &WorkspaceMember) -> Result<PathBuf> {
        info!("Generating rustdoc JSON for crate: {}", member.name);

        // Rustdoc outputs to workspace root's target/doc directory
        let workspace_target_dir = self.workspace_root.join("target/doc");
        std::fs::create_dir_all(&workspace_target_dir)?;

        info!(
            "Running rustdoc for crate {} from directory {:?}",
            member.name, self.workspace_root
        );
        let output = Command::new("cargo")
            .current_dir(&self.workspace_root)
            .args(&[
                "+nightly",
                "rustdoc",
                "-p",
                &member.name,
                "--lib",
                "--",
                "-Z",
                "unstable-options",
                "--output-format",
                "json",
                "--document-private-items",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("rustdoc failed for crate {}: {}", member.name, stderr);
            return Err(IndexError::IndexingFailed(format!(
                "rustdoc failed for crate {}: {}",
                member.name, stderr
            )));
        }

        // The JSON file is in the workspace root's target/doc directory
        // Try the exact crate name first
        let json_file = workspace_target_dir.join(format!("{}.json", member.name));
        if json_file.exists() {
            let metadata = std::fs::metadata(&json_file)?;
            info!(
                "Found rustdoc JSON at {:?}, size: {} bytes",
                json_file,
                metadata.len()
            );
            return Ok(json_file);
        }

        // Try with underscores replaced by hyphens (common in crate names)
        let json_file_normalized =
            workspace_target_dir.join(format!("{}.json", member.name.replace("_", "-")));
        if json_file_normalized.exists() {
            let metadata = std::fs::metadata(&json_file_normalized)?;
            info!(
                "Found rustdoc JSON at {:?} (normalized variant), size: {} bytes",
                json_file_normalized,
                metadata.len()
            );
            return Ok(json_file_normalized);
        }

        // If none found, return error with all attempted paths
        Err(IndexError::IndexingFailed(format!(
            "rustdoc JSON not found at any of these paths: {:?}, {:?}",
            json_file, json_file_normalized
        )))
    }

    fn parse_rustdoc_json(&self, path: &Path) -> Result<Crate> {
        let content = std::fs::read_to_string(path)?;
        let krate: Crate = serde_json::from_str(&content)?;
        Ok(krate)
    }

    fn process_rustdoc_data(
        &mut self,
        krate: &Crate,
        crate_id: i64,
        crate_name: &str,
        stats: &mut IndexStats,
    ) -> Result<()> {
        // Build path map for all local ids -> path segments
        let mut path_map: HashMap<Id, Vec<String>> = HashMap::new();
        for (id, summary) in &krate.paths {
            if summary.crate_id == 0 {
                path_map.insert(id.clone(), summary.path.clone());
            }
        }
        let local_ids: HashSet<Id> = path_map.keys().cloned().collect();
        
        info!("Built path_map with {} entries for crate {}", path_map.len(), crate_name);
        
        // Debug: Check if we have paths for all indexed items
        let mut items_without_paths = 0;
        for (id, item) in &krate.index {
            if item.crate_id == 0 && !path_map.contains_key(id) {
                items_without_paths += 1;
                if let Some(name) = &item.name {
                    warn!("Item {} (id: {:?}) has no entry in paths", name, id);
                }
            }
        }
        if items_without_paths > 0 {
            warn!("{} items have no paths entries", items_without_paths);
        }

        // Track which items belong to which impl blocks
        let mut impl_context_map: HashMap<Id, (Id, Option<Id>)> = HashMap::new();

        // First pass: map items to their impl blocks
        for (id, item) in &krate.index {
            if item.crate_id != 0 {
                continue;
            }
            if let ItemEnum::Impl(imp) = &item.inner {
                // Map all items in this impl to their parent impl
                for item_id in &imp.items {
                    impl_context_map.insert(
                        item_id.clone(),
                        (id.clone(), imp.trait_.as_ref().and_then(|path| Some(path.id.clone()))),
                    );
                }
            }
        }

        info!("Processing {} items from rustdoc index", krate.index.len());
        let mut items_processed = 0;

        for (id, item) in &krate.index {
            // Filter: only process local crate items
            if item.crate_id != 0 {
                continue;
            }

            // Skip derive methods unless explicitly included
            if !self.include_derives {
                if let Some(name) = &item.name {
                    if is_derive_method(name) && self.is_method_item(&item.inner) {
                        continue;
                    }
                }
            }

            // Extract symbol information
            if let Some(symbol) = self.extract_symbol(
                item,
                id,
                crate_id,
                crate_name,
                &path_map,
                &impl_context_map,
                &local_ids,
                krate,
            )? {
                // Apply module/struct filtering
                if !self.should_process_symbol(&symbol) {
                    continue;
                }
                
                info!(
                    "Extracted symbol: {} ({}) with ID: {} path: {}",
                    symbol.name,
                    symbol.kind.as_str(),
                    symbol.symbol_id,
                    symbol.path
                );
                
                self.db.insert_symbol(&symbol)?;
                stats.symbols_indexed += 1;
                items_processed += 1;

                // Process impl blocks
                if let ItemEnum::Impl(imp) = &item.inner {
                    if let Some(span) = &item.span {
                        self.process_impl_block(imp, crate_id, span, crate_name)?;
                        stats.symbols_indexed += 1;
                    }
                }
            }
        }

        info!(
            "Processed {} items, extracted {} symbols",
            items_processed, stats.symbols_indexed
        );

        Ok(())
    }

    fn extract_symbol(
        &mut self,
        item: &Item,
        id: &Id,
        crate_id: i64,
        crate_name: &str,
        path_map: &HashMap<Id, Vec<String>>,
        impl_context_map: &HashMap<Id, (Id, Option<Id>)>,
        local_ids: &HashSet<Id>,
        krate: &Crate,
    ) -> Result<Option<Symbol>> {
        let name = match &item.name {
            Some(n) => n.clone(),
            None => return Ok(None),
        };

        // Determine kind and signature
        let (kind, signature) = match &item.inner {
            ItemEnum::Module(_) => (SymbolKind::Module, format!("mod {}", name)),
            ItemEnum::Struct(s) => {
                let generics_str = self.format_generics(&s.generics);
                (SymbolKind::Struct, format!("struct {}{}", name, generics_str))
            }
            ItemEnum::Enum(e) => {
                let generics_str = self.format_generics(&e.generics);
                (SymbolKind::Enum, format!("enum {}{}", name, generics_str))
            }
            ItemEnum::Trait(t) => {
                let generics_str = self.format_generics(&t.generics);
                (
                    SymbolKind::Trait,
                    format!(
                        "{}trait {}{}",
                        if t.is_unsafe { "unsafe " } else { "" },
                        name,
                        generics_str
                    ),
                )
            }
            ItemEnum::Function(f) => {
                let sig = self.format_function_signature(&name, &f.sig, &f.generics, &f.header);
                // Check if this function is inside an impl block (making it a method)
                let kind = if impl_context_map.contains_key(id) {
                    SymbolKind::Method
                } else {
                    SymbolKind::Fn
                };
                (kind, sig)
            }
            ItemEnum::TypeAlias(t) => {
                let generics_str = self.format_generics(&t.generics);
                (SymbolKind::TypeAlias, format!("type {}{}", name, generics_str))
            }
            ItemEnum::Constant { type_: _, const_: _ } => {
                (SymbolKind::Const, format!("const {}: _", name))
            }
            ItemEnum::Static(s) => {
                (
                    SymbolKind::Static,
                    format!(
                        "{}static {}: _",
                        if s.is_mutable { "mut " } else { "" },
                        name
                    ),
                )
            }
            ItemEnum::Impl(_) => (SymbolKind::Impl, "impl".to_string()),
            ItemEnum::Variant(_) => (SymbolKind::Variant, format!("{}", name)),
            ItemEnum::StructField(_) => (SymbolKind::Field, name.clone()),
            _ => return Ok(None),
        };

        let visibility = match &item.visibility {
            rustdoc_types::Visibility::Public => Visibility::Public,
            _ => Visibility::Private,
        };

        // Build the canonical path with module hierarchy
        let path = if let Some((impl_id, trait_id)) = impl_context_map.get(id) {
            // This item is inside an impl block
            if let Some(impl_item) = krate.index.get(impl_id) {
                if let ItemEnum::Impl(imp) = &impl_item.inner {
                    let for_type = self.extract_type_path(&imp.for_, path_map, local_ids);
                    if let Some(trait_id) = trait_id {
                        if let Some(trait_path) = path_map.get(trait_id) {
                            // Trait impl method: crate::Type::trait::method
                            format!("{}::{}::{}::{}", crate_name, for_type, trait_path.join("::"), name)
                        } else {
                            // Fallback
                            format!("{}::{}::{}", crate_name, for_type, name)
                        }
                    } else {
                        // Inherent impl method: crate::Type::method
                        format!("{}::{}::{}", crate_name, for_type, name)
                    }
                } else {
                    format!("{}::{}", crate_name, name)
                }
            } else {
                format!("{}::{}", crate_name, name)
            }
        } else if let Some(path_segments) = path_map.get(id) {
            // Use the full path from rustdoc
            let full_path = path_segments.join("::");
            info!("Using path from path_map for {}: {}", name, full_path);
            full_path
        } else {
            // Fallback - but first try to find if this item exists in paths with a different ID format
            // This can happen because rustdoc-types Id can be either String or Number
            let mut found_path = None;
            for (_path_id, path_segments) in path_map.iter() {
                if let Some(last_segment) = path_segments.last() {
                    if last_segment == &name {
                        // Found a path ending with our item name
                        found_path = Some(path_segments.join("::"));
                        info!("Found path for {} via name search: {}", name, found_path.as_ref().unwrap());
                        break;
                    }
                }
            }
            
            if let Some(path) = found_path {
                path
            } else {
                warn!("Path not found in path_map for item {} (id: {:?}), using basic fallback", name, id);
                format!("{}::{}", crate_name, name)
            }
        };

        // Get span information
        let span = item.span.as_ref().ok_or_else(|| {
            IndexError::IndexingFailed(format!("Item {} has no span information", name))
        })?;
        let file_path = self.workspace_root.join(&span.filename);
        
        // Ensure file is in database
        let file_id = if let Some(&fid) = self.file_cache.get(&span.filename.to_string_lossy().to_string()) {
            fid
        } else {
            let digest = if file_path.exists() {
                let content = std::fs::read(&file_path)?;
                compute_file_digest(&content)
            } else {
                "missing".to_string()
            };

            let fid = self.db.insert_file(crate_id, &span.filename.to_string_lossy(), &digest)?;
            self.file_cache.insert(span.filename.to_string_lossy().to_string(), fid);
            fid
        };

        let symbol_id = compute_symbol_id(
            &path,
            kind.as_str(),
            &span.filename.to_string_lossy(),
            span.begin.0 as u32,
            span.end.0 as u32,
        );

        // Detect implementation status for functions/methods
        let status = if matches!(kind, SymbolKind::Fn | SymbolKind::Method) {
            self.detect_implementation_status(&file_path, span)?
        } else {
            ImplementationStatus::Implemented
        };

        Ok(Some(Symbol {
            symbol_id,
            crate_id,
            file_id,
            path,
            name,
            kind,
            visibility,
            signature: signature.clone(),
            docs: item.docs.clone(),
            status,
            span_start: span.begin.0 as u32,
            span_end: span.end.0 as u32,
            def_hash: format!("{}", blake3::hash(signature.as_bytes()).to_hex()),
        }))
    }

    fn is_method_item(&self, inner: &ItemEnum) -> bool {
        matches!(inner, ItemEnum::Function(_))
    }

    fn process_impl_block(
        &mut self,
        imp: &rustdoc_types::Impl,
        crate_id: i64,
        span: &rustdoc_types::Span,
        crate_name: &str,
    ) -> Result<()> {
        // Extract the type being implemented for
        let for_path = match &imp.for_ {
            Type::ResolvedPath(path) => {
                format!("{}::{}", crate_name, path.id.0)
            }
            _ => "unknown".to_string(),
        };

        // Extract trait path if this is a trait impl
        let trait_path = imp.trait_.as_ref().map(|path| path.id.0.to_string());

        // Get or create file ID
        let file_id = if let Some(&fid) = self.file_cache.get(&span.filename.to_string_lossy().to_string()) {
            fid
        } else {
            let file_path = self.workspace_root.join(&span.filename);
            let digest = if file_path.exists() {
                let content = std::fs::read(&file_path)?;
                compute_file_digest(&content)
            } else {
                "missing".to_string()
            };

            let fid = self.db.insert_file(crate_id, &span.filename.to_string_lossy(), &digest)?;
            self.file_cache.insert(span.filename.to_string_lossy().to_string(), fid);
            fid
        };

        let impl_block = ImplBlock {
            id: 0, // Will be set by database
            for_path,
            trait_path,
            file_id,
            line_start: span.begin.0 as u32,
            line_end: span.end.0 as u32,
        };

        self.db.insert_impl(&impl_block)?;

        Ok(())
    }

    fn extract_type_path(&self, ty: &Type, path_map: &HashMap<Id, Vec<String>>, local_ids: &HashSet<Id>) -> String {
        match ty {
            Type::ResolvedPath(path) => {
                if let Some(path_segments) = path_map.get(&path.id) {
                    path_segments.join("::")
                } else if local_ids.contains(&path.id) {
                    format!("type_{}", path.id.0)
                } else {
                    "external".to_string()
                }
            }
            Type::Primitive(p) => p.clone(),
            Type::Generic(g) => g.clone(),
            _ => "unknown".to_string(),
        }
    }


    fn format_generics(&self, generics: &rustdoc_types::Generics) -> String {
        if generics.params.is_empty() {
            return String::new();
        }

        let params: Vec<String> = generics
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect();

        format!("<{}>", params.join(", "))
    }

    fn format_function_signature(
        &self,
        name: &str,
        sig: &rustdoc_types::FunctionSignature,
        generics: &rustdoc_types::Generics,
        header: &rustdoc_types::FunctionHeader,
    ) -> String {
        let mut result = String::new();

        // Add qualifiers
        if header.is_const {
            result.push_str("const ");
        }
        if header.is_async {
            result.push_str("async ");
        }
        if header.is_unsafe {
            result.push_str("unsafe ");
        }

        result.push_str("fn ");
        result.push_str(name);
        result.push_str(&self.format_generics(generics));
        result.push('(');

        // Add parameters
        for (i, (param_name, _param_type)) in sig.inputs.iter().enumerate() {
            if i > 0 {
                result.push_str(", ");
            }
            result.push_str(param_name);
        }

        result.push(')');

        // Add return type
        if let Some(_output) = &sig.output {
            result.push_str(" -> _");
        }

        result
    }

    fn detect_implementation_status(
        &self,
        file_path: &Path,
        span: &rustdoc_types::Span,
    ) -> Result<ImplementationStatus> {
        // Read the file content for the span
        if let Ok(content) = std::fs::read_to_string(file_path) {
            // Get lines for the span
            let lines: Vec<&str> = content.lines().collect();
            let start_line = span.begin.0.saturating_sub(1);
            let end_line = span.end.0.min(lines.len());

            if start_line >= lines.len() {
                return Ok(ImplementationStatus::Implemented);
            }

            // Check the function body for unimplemented! or todo!
            let body_text = lines[start_line..end_line].join("\n");

            // Look for unimplemented!() macro
            if body_text.contains("unimplemented!") {
                return Ok(ImplementationStatus::Unimplemented);
            }

            // Look for todo!() macro or TODO/FIXME comments
            if body_text.contains("todo!")
                || body_text.contains("TODO")
                || body_text.contains("FIXME")
            {
                return Ok(ImplementationStatus::Todo);
            }
        }

        Ok(ImplementationStatus::Implemented)
    }

    fn should_process_symbol(&self, symbol: &Symbol) -> bool {
        // If no filters specified, process everything
        if self.filter_module.is_none() && self.filter_struct.is_none() {
            return true;
        }

        // Check module filter
        if let Some(module) = &self.filter_module {
            if !symbol.path.starts_with(module) {
                return false;
            }
        }

        // Check struct filter
        if let Some(struct_name) = &self.filter_struct {
            if let Some(module) = &self.filter_module {
                let expected_path = format!("{}::{}", module, struct_name);
                if !symbol.path.starts_with(&expected_path) {
                    return false;
                }
            } else if !symbol.path.contains(&format!("::{}", struct_name)) {
                return false;
            }
        }

        true
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