use anyhow::{Context, Result};
use clap::Parser;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use rustdoc_types::{Crate, Id, ItemEnum, Type};

// Common derive trait methods to filter out
const DERIVE_METHODS: &[&str] = &[
    "clone", "clone_from", "fmt", "eq", "ne", "partial_cmp", "cmp",
    "hash", "serialize", "deserialize", "default", "from", "into",
    "try_from", "try_into", "as_ref", "as_mut", "borrow", "borrow_mut",
    "to_owned", "to_string", "drop", "deref", "deref_mut"
];

fn is_derive_method(method_name: &str) -> bool {
    DERIVE_METHODS.contains(&method_name)
}

#[derive(Parser)]
pub struct RustdocParseArgs {
    /// Module path to filter graph output (e.g. "crate::module")
    #[arg(long)]
    pub module: Option<String>,
    
    /// Struct within the module to focus on
    #[arg(long, name = "struct")]
    pub struct_name: Option<String>,

    /// Hide tasks that have no connections
    #[arg(long)]
    pub no_orphan: bool,
    
    /// Include derive trait implementations (clone, serialize, etc.)
    #[arg(long)]
    pub include_derives: bool,
}

pub struct ParsedRustdoc {
    pub items: Vec<ParsedItem>,
    pub relationships: Vec<Relationship>,
}

#[derive(Debug, Clone)]
pub struct ParsedItem {
    pub id: Id,
    pub path: String,
    pub name: String,
    pub kind: ItemKind,
    pub docs: Option<String>,
    pub visibility: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Module,
    Struct,
    Enum,
    Trait,
    Function,
    Method,
    Const,
    Static,
    TypeAlias,
    Impl,
    Field,
    Variant,
}

#[derive(Debug, Clone)]
pub struct Relationship {
    pub from: Id,
    pub to: Id,
    pub kind: RelationshipKind,
}

#[derive(Debug, Clone)]
pub enum RelationshipKind {
    StructField,
    ImplFor,
    TraitImpl,
    MethodOf,
    VariantOf,
}

pub fn parse_rustdoc_with_filters(
    json_path: &PathBuf,
    args: &RustdocParseArgs,
) -> Result<ParsedRustdoc> {
    let data = fs::read_to_string(json_path)
        .with_context(|| format!("reading {:?}", json_path))?;
    let krate: Crate = serde_json::from_str(&data)?;

    // Map of all local ids -> path segments
    let mut path_map: HashMap<Id, Vec<String>> = HashMap::new();
    for (id, summary) in &krate.paths {
        if summary.crate_id == 0 {  // Only local crate items
            path_map.insert(id.clone(), summary.path.clone());
        }
    }
    let local_ids: HashSet<Id> = path_map.keys().cloned().collect();

    let mut items = Vec::new();
    let mut relationships = Vec::new();
    let mut seen_modules: HashSet<String> = HashSet::new();

    // Process all items in the index
    for (id, item) in &krate.index {
        // Filter: only process local crate items
        if item.crate_id != 0 {
            continue;
        }

        // Extract item details based on type
        match &item.inner {
            ItemEnum::Module(_) => {
                let path = get_full_path(&path_map, id, &item.name);
                if should_process_item(&path, args) {
                    items.push(ParsedItem {
                        id: id.clone(),
                        path: path.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        kind: ItemKind::Module,
                        docs: item.docs.clone(),
                        visibility: format!("{:?}", item.visibility),
                    });
                    seen_modules.insert(path);
                }
            }
            ItemEnum::Struct(s) => {
                let path = get_full_path(&path_map, id, &item.name);
                if should_process_item(&path, args) {
                    items.push(ParsedItem {
                        id: id.clone(),
                        path: path.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        kind: ItemKind::Struct,
                        docs: item.docs.clone(),
                        visibility: format!("{:?}", item.visibility),
                    });

                    // Process struct fields
                    process_struct_fields(id, s, &krate, &mut relationships, &local_ids);
                }
            }
            ItemEnum::Enum(e) => {
                let path = get_full_path(&path_map, id, &item.name);
                if should_process_item(&path, args) {
                    items.push(ParsedItem {
                        id: id.clone(),
                        path: path.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        kind: ItemKind::Enum,
                        docs: item.docs.clone(),
                        visibility: format!("{:?}", item.visibility),
                    });

                    // Process enum variants
                    for variant_id in &e.variants {
                        if local_ids.contains(variant_id) {
                            relationships.push(Relationship {
                                from: id.clone(),
                                to: variant_id.clone(),
                                kind: RelationshipKind::VariantOf,
                            });
                        }
                    }
                }
            }
            ItemEnum::Trait(t) => {
                let path = get_full_path(&path_map, id, &item.name);
                if should_process_item(&path, args) {
                    items.push(ParsedItem {
                        id: id.clone(),
                        path: path.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        kind: ItemKind::Trait,
                        docs: item.docs.clone(),
                        visibility: format!("{:?}", item.visibility),
                    });

                    // Process trait methods
                    for method_id in &t.items {
                        if let Some(method_item) = krate.index.get(method_id) {
                            if let Some(method_name) = &method_item.name {
                                if !args.include_derives && is_derive_method(method_name) {
                                    continue;
                                }
                                if local_ids.contains(method_id) {
                                    relationships.push(Relationship {
                                        from: id.clone(),
                                        to: method_id.clone(),
                                        kind: RelationshipKind::MethodOf,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            ItemEnum::Function(_) => {
                if let Some(method_name) = &item.name {
                    if !args.include_derives && is_derive_method(method_name) {
                        continue;
                    }
                }
                let path = get_full_path(&path_map, id, &item.name);
                if should_process_item(&path, args) {
                    items.push(ParsedItem {
                        id: id.clone(),
                        path: path.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        kind: ItemKind::Function,
                        docs: item.docs.clone(),
                        visibility: format!("{:?}", item.visibility),
                    });
                }
            }
            ItemEnum::Impl(imp) => {
                // Process impl blocks to establish relationships
                if let Type::ResolvedPath(p) = &imp.for_ {
                    if local_ids.contains(&p.id) {
                        for item_id in &imp.items {
                            if let Some(method_item) = krate.index.get(item_id) {
                                if let Some(method_name) = &method_item.name {
                                    if !args.include_derives && is_derive_method(method_name) {
                                        continue;
                                    }
                                }
                                if local_ids.contains(item_id) {
                                    relationships.push(Relationship {
                                        from: p.id.clone(),
                                        to: item_id.clone(),
                                        kind: RelationshipKind::MethodOf,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            ItemEnum::Constant(_) => {
                let path = get_full_path(&path_map, id, &item.name);
                if should_process_item(&path, args) {
                    items.push(ParsedItem {
                        id: id.clone(),
                        path,
                        name: item.name.clone().unwrap_or_default(),
                        kind: ItemKind::Const,
                        docs: item.docs.clone(),
                        visibility: format!("{:?}", item.visibility),
                    });
                }
            }
            ItemEnum::Static(_) => {
                let path = get_full_path(&path_map, id, &item.name);
                if should_process_item(&path, args) {
                    items.push(ParsedItem {
                        id: id.clone(),
                        path,
                        name: item.name.clone().unwrap_or_default(),
                        kind: ItemKind::Static,
                        docs: item.docs.clone(),
                        visibility: format!("{:?}", item.visibility),
                    });
                }
            }
            ItemEnum::TypeAlias(_) => {
                let path = get_full_path(&path_map, id, &item.name);
                if should_process_item(&path, args) {
                    items.push(ParsedItem {
                        id: id.clone(),
                        path,
                        name: item.name.clone().unwrap_or_default(),
                        kind: ItemKind::TypeAlias,
                        docs: item.docs.clone(),
                        visibility: format!("{:?}", item.visibility),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(ParsedRustdoc {
        items,
        relationships,
    })
}

fn get_full_path(
    path_map: &HashMap<Id, Vec<String>>,
    id: &Id,
    name: &Option<String>,
) -> String {
    path_map
        .get(id)
        .map(|v| v.join("::"))
        .unwrap_or_else(|| name.clone().unwrap_or_default())
}

fn should_process_item(path: &str, args: &RustdocParseArgs) -> bool {
    // If no filters specified, process everything
    if args.module.is_none() && args.struct_name.is_none() {
        return true;
    }

    // Check module filter
    if let Some(module) = &args.module {
        if !path.starts_with(module) {
            return false;
        }
    }

    // Check struct filter
    if let Some(struct_name) = &args.struct_name {
        if let Some(module) = &args.module {
            let expected_path = format!("{}::{}", module, struct_name);
            if !path.starts_with(&expected_path) {
                return false;
            }
        } else if !path.contains(&format!("::{}", struct_name)) {
            return false;
        }
    }

    true
}

fn process_struct_fields(
    struct_id: &Id,
    strct: &rustdoc_types::Struct,
    krate: &Crate,
    relationships: &mut Vec<Relationship>,
    local_ids: &HashSet<Id>,
) {
    use rustdoc_types::StructKind::*;
    
    let field_ids: Vec<Id> = match &strct.kind {
        Plain { fields, .. } => fields.clone(),
        Tuple(fields) => fields.iter().filter_map(|f| f.clone()).collect(),
        Unit => Vec::new(),
    };

    for field_id in field_ids {
        if let Some(field_item) = krate.index.get(&field_id) {
            if let ItemEnum::StructField(ty) = &field_item.inner {
                let mut referenced = Vec::new();
                collect_local_ids_from_type(ty, &mut referenced, local_ids);
                
                for ref_id in referenced {
                    relationships.push(Relationship {
                        from: struct_id.clone(),
                        to: ref_id,
                        kind: RelationshipKind::StructField,
                    });
                }
            }
        }
    }
}

fn collect_local_ids_from_type(
    ty: &Type,
    out: &mut Vec<Id>,
    local_ids: &HashSet<Id>,
) {
    use rustdoc_types::Type::*;
    
    match ty {
        ResolvedPath(p) => {
            if local_ids.contains(&p.id) {
                out.push(p.id.clone());
            }
            if let Some(args) = &p.args {
                collect_local_ids_from_generic_args(args, out, local_ids);
            }
        }
        Tuple(ts) => {
            for t in ts {
                collect_local_ids_from_type(t, out, local_ids);
            }
        }
        Slice(t) => collect_local_ids_from_type(t, out, local_ids),
        Array { type_, .. } => collect_local_ids_from_type(type_, out, local_ids),
        BorrowedRef { type_, .. } => collect_local_ids_from_type(type_, out, local_ids),
        RawPointer { type_, .. } => collect_local_ids_from_type(type_, out, local_ids),
        QualifiedPath { self_type, trait_, .. } => {
            collect_local_ids_from_type(self_type, out, local_ids);
            if let Some(tr) = trait_ {
                if local_ids.contains(&tr.id) {
                    out.push(tr.id.clone());
                }
                if let Some(a) = &tr.args {
                    collect_local_ids_from_generic_args(a, out, local_ids);
                }
            }
        }
        ImplTrait(bounds) => {
            for b in bounds {
                collect_local_ids_from_generic_bound(b, out, local_ids);
            }
        }
        _ => {}
    }
}

fn collect_local_ids_from_generic_args(
    args: &rustdoc_types::GenericArgs,
    out: &mut Vec<Id>,
    local_ids: &HashSet<Id>,
) {
    use rustdoc_types::{GenericArg, GenericArgs};
    
    match args {
        GenericArgs::AngleBracketed { args, bindings } => {
            for a in args {
                if let GenericArg::Type(t) = a {
                    collect_local_ids_from_type(t, out, local_ids);
                }
            }
            for b in bindings {
                use rustdoc_types::TypeBindingKind::*;
                match &b.binding {
                    Equality(term) => {
                        if let rustdoc_types::Term::Type(t) = term {
                            collect_local_ids_from_type(t, out, local_ids);
                        }
                    }
                    Constraint(bounds) => {
                        for bound in bounds {
                            collect_local_ids_from_generic_bound(bound, out, local_ids);
                        }
                    }
                }
            }
        }
        GenericArgs::Parenthesized { inputs, output } => {
            for t in inputs {
                collect_local_ids_from_type(t, out, local_ids);
            }
            if let Some(t) = output {
                collect_local_ids_from_type(t, out, local_ids);
            }
        }
    }
}

fn collect_local_ids_from_generic_bound(
    bound: &rustdoc_types::GenericBound,
    out: &mut Vec<Id>,
    local_ids: &HashSet<Id>,
) {
    use rustdoc_types::GenericBound::*;
    
    match bound {
        TraitBound { trait_, .. } => {
            if local_ids.contains(&trait_.id) {
                out.push(trait_.id.clone());
            }
            if let Some(args) = &trait_.args {
                collect_local_ids_from_generic_args(args, out, local_ids);
            }
        }
        _ => {}
    }
}