use ct_core::config::Config;
use ct_core::models::*;
use ct_protocol::{Request, Response, Command, ErrorCode, PROTOCOL_VERSION};
use ct_db::{Database, queries};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
use serde_json::json;

pub struct DaemonState {
    config: Config,
    workspace_fingerprint: String,
    db_path: PathBuf,
    index_timestamp: SystemTime,
    last_index_duration_ms: u64,
}

impl DaemonState {
    pub fn new(config: Config, workspace_fingerprint: String) -> Self {
        let db_path = config.get_db_path(&workspace_fingerprint);
        
        Self {
            config,
            workspace_fingerprint,
            db_path,
            index_timestamp: SystemTime::now(),
            last_index_duration_ms: 0,
        }
    }

    pub async fn handle_request(&mut self, request: Request) -> Response {
        let start = std::time::Instant::now();
        
        let result = match request.cmd {
            Command::Find { name, path, kind, vis, unimplemented, todo, all } => {
                self.handle_find(name, path, kind, vis, unimplemented, todo, all).await
            }
            Command::Doc { path, include_docs, vis, unimplemented, todo } => {
                self.handle_doc(path, include_docs, vis, unimplemented, todo).await
            }
            Command::Ls { path, expansion, impl_parents, include_docs, vis, unimplemented, todo } => {
                self.handle_ls(path, expansion, impl_parents, include_docs, vis, unimplemented, todo).await
            }
            Command::Export { path, bundle, expansion, include_docs, vis, unimplemented, todo, impl_parents, with_source } => {
                self.handle_export(path, bundle, expansion, include_docs, vis, unimplemented, todo, impl_parents, with_source).await
            }
            Command::Reindex { features, target, module, struct_name, include_derives } => {
                self.handle_reindex(features, target, module, struct_name, include_derives).await
            }
            Command::Status { vis, unimplemented, todo } => {
                self.handle_status(vis, unimplemented, todo).await
            }
            Command::Diag => {
                self.handle_diag().await
            }
            Command::Bench { queries, warmup, duration } => {
                self.handle_bench(queries, warmup, duration).await
            }
        };
        
        let elapsed_ms = start.elapsed().as_millis() as u64;
        
        match result {
            Ok(mut response) => {
                if let Response::Success(ref mut envelope) = response {
                    envelope.metrics = Some(ct_protocol::Metrics {
                        elapsed_ms,
                        bytes: 0, // TODO: Calculate actual response size
                    });
                }
                response
            }
            Err((err_msg, err_code)) => {
                Response::error(request.request_id, err_msg, err_code)
            }
        }
    }

    async fn handle_find(
        &self,
        name: Option<String>,
        path: Option<String>,
        kind: Option<String>,
        vis: Option<String>,
        unimplemented: Option<bool>,
        todo: Option<bool>,
        all: Option<bool>,
    ) -> Result<Response, (String, ErrorCode)> {
        if name.is_none() && path.is_none() {
            return Err(("Must provide either name or path".to_string(), ErrorCode::InvalidArg));
        }
        
        let db = Database::open(&self.db_path)
            .map_err(|e| (format!("Database error: {}", e), ErrorCode::InternalError))?;
        
        let symbols = if let Some(name) = name {
            let status_filter = match (unimplemented, todo) {
                (Some(true), Some(true)) => None, // Show both
                (Some(true), _) => Some("unimplemented"),
                (_, Some(true)) => Some("todo"),
                _ => Some("implemented"),
            };
            
            queries::find_symbols_by_name(
                db.conn(),
                &name,
                kind.as_deref(),
                vis.as_deref(),
                status_filter,
                self.config.max_list,
            ).map_err(|e| (format!("Query error: {}", e), ErrorCode::InternalError))?
        } else if let Some(_path) = path {
            vec![]  // TODO: Implement path search
        } else {
            vec![]
        };
        
        // Filter response based on 'all' flag
        let items: Vec<serde_json::Value> = if all.unwrap_or(false) {
            // Return all fields
            symbols.into_iter().map(|s| serde_json::to_value(s).unwrap()).collect()
        } else {
            // Return only path and span fields
            symbols.into_iter().map(|s| {
                json!({
                    "path": s.path,
                    "span_start": s.span_start,
                    "span_end": s.span_end,
                })
            }).collect()
        };
        
        Ok(Response::success(
            "".to_string(), // Request ID will be filled by caller
            json!({
                "items": items,
            }),
        ))
    }

    async fn handle_doc(
        &self,
        path: String,
        include_docs: bool,
        _vis: Option<String>,
        _unimplemented: Option<bool>,
        _todo: Option<bool>,
    ) -> Result<Response, (String, ErrorCode)> {
        // Stub implementation
        Ok(Response::success(
            "".to_string(),
            json!({
                "symbol": {
                    "path": path,
                    "signature": "pub struct Example",
                    "docs": if include_docs { Some("Example documentation") } else { None },
                },
            }),
        ))
    }

    async fn handle_ls(
        &self,
        _path: String,
        _expansion: Option<String>,
        _impl_parents: bool,
        _include_docs: bool,
        _vis: Option<String>,
        _unimplemented: Option<bool>,
        _todo: Option<bool>,
    ) -> Result<Response, (String, ErrorCode)> {
        // Stub implementation
        Ok(Response::success(
            "".to_string(),
            json!({
                "items": [],
            }),
        ))
    }

    async fn handle_export(
        &self,
        path: String,
        _bundle: bool,
        _expansion: Option<String>,
        _include_docs: bool,
        _vis: Option<String>,
        _unimplemented: Option<bool>,
        _todo: Option<bool>,
        _impl_parents: bool,
        _with_source: bool,
    ) -> Result<Response, (String, ErrorCode)> {
        // Stub implementation
        Ok(Response::success(
            "".to_string(),
            json!({
                "bundle": {
                    "symbol": {
                        "path": path,
                        "kind": "struct",
                        "signature": "pub struct Example",
                    },
                    "children": [],
                    "extern_refs": [],
                    "impl_ranges": [],
                    "order": "bfs",
                    "invariants": {
                        "range_1_based_inclusive": true,
                    },
                },
            }),
        ))
    }

    async fn handle_reindex(
        &self,
        features: Option<Vec<String>>,
        target: Option<String>,
        module: Option<String>,
        struct_name: Option<String>,
        include_derives: bool,
    ) -> Result<Response, (String, ErrorCode)> {
        // Stub implementation
        info!("Reindexing requested with features: {:?}, target: {:?}, module: {:?}, struct: {:?}, include_derives: {}", 
              features, target, module, struct_name, include_derives);
        
        // TODO: Pass filtering options to the indexer when reindexing
        // let mut indexer = Indexer::new(workspace_root, db)
        //     .with_filters(module, struct_name, include_derives);
        
        Ok(Response::success(
            "".to_string(),
            json!({
                "status": "reindex_started",
                "filters": {
                    "module": module,
                    "struct_name": struct_name,
                    "include_derives": include_derives
                }
            }),
        ))
    }

    async fn handle_status(
        &self,
        vis: Option<String>,
        unimplemented: Option<bool>,
        todo: Option<bool>,
    ) -> Result<Response, (String, ErrorCode)> {
        let db = Database::open(&self.db_path)
            .map_err(|e| (format!("Database error: {}", e), ErrorCode::InternalError))?;
        
        let counts = queries::get_status_counts(db.conn(), vis.as_deref())
            .map_err(|e| (format!("Query error: {}", e), ErrorCode::InternalError))?;
        
        let items = queries::get_status_items(
            db.conn(),
            vis.as_deref(),
            unimplemented.unwrap_or(false),
            todo.unwrap_or(false),
            self.config.max_list,
        ).map_err(|e| (format!("Query error: {}", e), ErrorCode::InternalError))?;
        
        Ok(Response::success(
            "".to_string(),
            json!({
                "counts": counts,
                "items": items,
            }),
        ))
    }

    async fn handle_diag(&self) -> Result<Response, (String, ErrorCode)> {
        let db = Database::open(&self.db_path)
            .map_err(|e| (format!("Database error: {}", e), ErrorCode::InternalError))?;
        
        let symbol_count = db.get_symbol_count()
            .map_err(|e| (format!("Query error: {}", e), ErrorCode::InternalError))?;
        let crate_count = db.get_crate_count()
            .map_err(|e| (format!("Query error: {}", e), ErrorCode::InternalError))?;
        let file_count = db.get_file_count()
            .map_err(|e| (format!("Query error: {}", e), ErrorCode::InternalError))?;
        
        let timestamp = self.index_timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let diag = DiagResponse {
            db_path: self.db_path.to_string_lossy().to_string(),
            schema_version: "1".to_string(),
            tool_version: "0.1.0".to_string(),
            protocol_versions_supported: vec![PROTOCOL_VERSION],
            workspace_root: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            workspace_fingerprint: self.workspace_fingerprint.clone(),
            crate_count,
            file_count,
            symbol_count,
            mem_footprint_bytes: 0, // TODO: Implement memory tracking
            last_index_duration_ms: self.last_index_duration_ms,
            index_timestamp: chrono::DateTime::from_timestamp(timestamp as i64, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            rustc_hash: "sha256:unknown".to_string(), // TODO: Get actual rustc hash
            features: vec![],
            target: "x86_64-unknown-linux-gnu".to_string(), // TODO: Get actual target
            daemon_hot: true,
            transport: format!("{:?}", self.config.get_effective_transport()).to_lowercase(),
        };
        
        Ok(Response::success(
            "".to_string(),
            serde_json::to_value(diag).unwrap(),
        ))
    }

    async fn handle_bench(
        &self,
        queries: u32,
        warmup: u32,
        duration: u32,
    ) -> Result<Response, (String, ErrorCode)> {
        // Stub implementation
        info!("Benchmarking with {} queries, {}ms warmup, {}s duration", queries, warmup, duration);
        Ok(Response::success(
            "".to_string(),
            json!({
                "query_latency_p50_ms": 5,
                "query_latency_p90_ms": 10,
                "query_latency_p99_ms": 20,
                "throughput_qps": 200,
                "configuration": {
                    "queries": queries,
                    "warmup_ms": warmup,
                    "duration_s": duration,
                },
            }),
        ))
    }
}