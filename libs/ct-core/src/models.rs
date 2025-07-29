use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub symbol_id: String,
    pub crate_id: i64,
    pub file_id: i64,
    pub path: String,
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    pub status: ImplementationStatus,
    pub span_start: u32,
    pub span_end: u32,
    pub def_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Module,
    Struct,
    Enum,
    Trait,
    Fn,
    Method,
    Field,
    Variant,
    TypeAlias,
    Const,
    Static,
    Impl,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Fn => "fn",
            Self::Method => "method",
            Self::Field => "field",
            Self::Variant => "variant",
            Self::TypeAlias => "type_alias",
            Self::Const => "const",
            Self::Static => "static",
            Self::Impl => "impl",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImplementationStatus {
    Implemented,
    Unimplemented,
    Todo,
}

impl ImplementationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::Unimplemented => "unimplemented",
            Self::Todo => "todo",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crate {
    pub id: i64,
    pub name: String,
    pub version: Option<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub id: i64,
    pub crate_id: i64,
    pub path: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplBlock {
    pub id: i64,
    pub for_path: String,
    pub trait_path: Option<String>,
    pub file_id: i64,
    pub line_start: u32,
    pub line_end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub id: i64,
    pub symbol_id: i64,
    pub target_path: String,
    pub file_id: i64,
    pub span_start: u32,
    pub span_end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub symbol: Symbol,
    #[serde(default)]
    pub children: Vec<Symbol>,
    #[serde(default)]
    pub extern_refs: Vec<String>,
    #[serde(default)]
    pub impl_ranges: Vec<ImplRange>,
    pub order: String,
    pub invariants: BundleInvariants,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplRange {
    pub file: String,
    pub file_digest: String,
    pub line_start: u32,
    pub line_end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleInvariants {
    pub range_1_based_inclusive: bool,
}

impl Default for BundleInvariants {
    fn default() -> Self {
        Self {
            range_1_based_inclusive: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusCounts {
    pub total: usize,
    pub implemented: usize,
    pub unimplemented: usize,
    pub todo: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusItem {
    pub path: String,
    pub status: ImplementationStatus,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub counts: StatusCounts,
    #[serde(default)]
    pub items: Vec<StatusItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagResponse {
    pub db_path: String,
    pub schema_version: String,
    pub tool_version: String,
    pub protocol_versions_supported: Vec<u32>,
    pub workspace_root: String,
    pub workspace_fingerprint: String,
    pub crate_count: usize,
    pub file_count: usize,
    pub symbol_count: usize,
    pub mem_footprint_bytes: usize,
    pub last_index_duration_ms: u64,
    pub index_timestamp: String,
    pub rustc_hash: String,
    pub features: Vec<String>,
    pub target: String,
    pub daemon_hot: bool,
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindResult {
    pub items: Vec<Symbol>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_str() {
        assert_eq!(SymbolKind::Struct.as_str(), "struct");
        assert_eq!(SymbolKind::Fn.as_str(), "fn");
    }

    #[test]
    fn test_visibility_str() {
        assert_eq!(Visibility::Public.as_str(), "public");
        assert_eq!(Visibility::Private.as_str(), "private");
    }
}