use anyhow::{Context, Result};
use clap::Parser;
use planner::{Plan, Task, TaskId, TaskKind, TaskStatus, DEFAULT_WEIGHT};
use rustdoc_types::{Crate, Id, ItemEnum, Type};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

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

#[derive(Parser)]
struct Cli {
    /// Module path to filter graph output (e.g. "bb_plan::engine")
    #[arg(long)]
    module: Option<String>,
    /// Struct within the module to focus on
    #[arg(long, name = "struct")]
    struct_name: Option<String>,

    /// Hide tasks that have no connections
    #[arg(long)]
    no_orphan: bool,

    /// Include derive trait implementations (clone, serialize, etc.)
    #[arg(long)]
    include_derives: bool,

    /// Show execution time
    #[arg(long)]
    time: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let start = if cli.time { Some(Instant::now()) } else { None };

    let json_path = PathBuf::from("target/doc/bb_plan.json");
    run_rustdoc()?;

    let data =
        fs::read_to_string(&json_path).with_context(|| format!("reading {:?}", json_path))?;
    let krate: Crate = serde_json::from_str(&data)?;

    // Map of all local ids -> path segments
    let mut path_map: HashMap<Id, Vec<String>> = HashMap::new();
    for (id, summary) in &krate.paths {
        if summary.crate_id == 0 {
            path_map.insert(*id, summary.path.clone());
        }
    }
    let local_ids: HashSet<Id> = path_map.keys().cloned().collect();

    let mut plan = Plan::new();
    let mut id_to_task: HashMap<Id, TaskId> = HashMap::new();
    let mut name_to_task: HashMap<String, TaskId> = HashMap::new();

    // Track modules we've seen
    let mut seen_modules: HashSet<String> = HashSet::new();

    // ------------------------------------------------------------------
    // Create tasks for structs and functions
    // ------------------------------------------------------------------
    eprintln!("Analyzing crate items...");
    let mut struct_count = 0;
    let mut enum_count = 0;
    let mut trait_count = 0;
    let mut function_count = 0;

    for (id, item) in &krate.index {
        if item.crate_id != 0 {
            continue;
        }
        match &item.inner {
            ItemEnum::Struct(_) => {
                let name = path_map
                    .get(id)
                    .map(|v| v.join("::"))
                    .unwrap_or_else(|| item.name.clone().unwrap_or_default());

                // Extract module from the full path
                if let Some(_module) = name.rsplit("::").nth(1) {
                    let module_path = name.rsplitn(2, "::").nth(1).unwrap_or("");
                    if seen_modules.insert(module_path.to_string()) {
                        eprintln!("  Exploring module: {}", module_path);
                    }
                }

                eprintln!("    Found struct: {}", name);
                struct_count += 1;

                let tid = if let Some(&tid) = name_to_task.get(&name) {
                    tid
                } else {
                    let t = plan.add_task(Task {
                        name: name.clone(),
                        description: item.docs.clone().unwrap_or_default(),
                        kind: TaskKind::Struct,
                        status: TaskStatus::NotStarted,
                        weight: DEFAULT_WEIGHT,
                    });
                    name_to_task.insert(name.clone(), t);
                    t
                };
                id_to_task.insert(*id, tid);
            }
            ItemEnum::Enum(_) => {
                let name = path_map
                    .get(id)
                    .map(|v| v.join("::"))
                    .unwrap_or_else(|| item.name.clone().unwrap_or_default());

                // Extract module from the full path
                if let Some(_module) = name.rsplit("::").nth(1) {
                    let module_path = name.rsplitn(2, "::").nth(1).unwrap_or("");
                    if seen_modules.insert(module_path.to_string()) {
                        eprintln!("  Exploring module: {}", module_path);
                    }
                }

                eprintln!("    Found enum: {}", name);
                enum_count += 1;

                let tid = if let Some(&tid) = name_to_task.get(&name) {
                    tid
                } else {
                    let t = plan.add_task(Task {
                        name: name.clone(),
                        description: item.docs.clone().unwrap_or_default(),
                        kind: TaskKind::Enum,
                        status: TaskStatus::NotStarted,
                        weight: DEFAULT_WEIGHT,
                    });
                    name_to_task.insert(name.clone(), t);
                    t
                };
                id_to_task.insert(*id, tid);
            }
            ItemEnum::Trait(_) => {
                let name = path_map
                    .get(id)
                    .map(|v| v.join("::"))
                    .unwrap_or_else(|| item.name.clone().unwrap_or_default());

                // Extract module from the full path
                if let Some(_module) = name.rsplit("::").nth(1) {
                    let module_path = name.rsplitn(2, "::").nth(1).unwrap_or("");
                    if seen_modules.insert(module_path.to_string()) {
                        eprintln!("  Exploring module: {}", module_path);
                    }
                }

                eprintln!("    Found trait: {}", name);
                trait_count += 1;

                let tid = if let Some(&tid) = name_to_task.get(&name) {
                    tid
                } else {
                    let t = plan.add_task(Task {
                        name: name.clone(),
                        description: item.docs.clone().unwrap_or_default(),
                        kind: TaskKind::Trait,
                        status: TaskStatus::NotStarted,
                        weight: DEFAULT_WEIGHT,
                    });
                    name_to_task.insert(name.clone(), t);
                    t
                };
                id_to_task.insert(*id, tid);
            }
            ItemEnum::Function(_) => {
                // Skip derive methods unless explicitly included
                if !cli.include_derives {
                    if let Some(method_name) = item.name.as_ref() {
                        if is_derive_method(method_name) {
                            continue;
                        }
                    }
                }

                let (mut name, from_paths) = match path_map.get(id) {
                    Some(path) => (path.join("::"), true),
                    None => (item.name.clone().unwrap_or_default(), false),
                };
                if !from_paths {
                    name = format!("{}#{}", name, id.0);
                }

                // Only log top-level functions (not methods)
                if !name.contains("::lib::") || name.matches("::").count() <= 3 {
                    function_count += 1;
                }

                let tid = if let Some(&tid) = name_to_task.get(&name) {
                    tid
                } else {
                    let t = plan.add_task(Task {
                        name: name.clone(),
                        description: item.docs.clone().unwrap_or_default(),
                        kind: TaskKind::Function,
                        status: TaskStatus::NotStarted,
                        weight: DEFAULT_WEIGHT,
                    });
                    name_to_task.insert(name.clone(), t);
                    t
                };
                id_to_task.insert(*id, tid);
            }
            _ => {}
        }
    }

    eprintln!("\nSummary of items found:");
    eprintln!("  Structs: {}", struct_count);
    eprintln!("  Enums: {}", enum_count);
    eprintln!("  Traits: {}", trait_count);
    eprintln!("  Functions: {}", function_count);
    eprintln!("  Total modules: {}", seen_modules.len());
    eprintln!();

    // ------------------------------------------------------------------
    // Link structs to referenced field types
    // ------------------------------------------------------------------
    eprintln!("Analyzing struct field dependencies...");
    for (id, item) in &krate.index {
        if item.crate_id != 0 {
            continue;
        }
        if let ItemEnum::Struct(strct) = &item.inner {
            if let Some(&parent_tid) = id_to_task.get(id) {
                let parent_name = path_map
                    .get(id)
                    .map(|v| v.join("::"))
                    .unwrap_or_else(|| item.name.clone().unwrap_or_default());

                use rustdoc_types::StructKind::*;
                let field_ids: Vec<Id> = match &strct.kind {
                    Plain { fields, .. } => fields.clone(),
                    Tuple(fields) => fields.iter().filter_map(|f| *f).collect(),
                    Unit => Vec::new(),
                };
                for fid in field_ids {
                    if let Some(field_item) = krate.index.get(&fid) {
                        if let ItemEnum::StructField(ty) = &field_item.inner {
                            let mut referenced = Vec::new();
                            collect_local_ids_from_type(&path_map, ty, &mut referenced);
                            for rid in referenced {
                                if let Some(&child_tid) = id_to_task.get(&rid) {
                                    let child_name = plan.task(child_tid).name.clone();
                                    let field_name = field_item
                                        .name
                                        .clone()
                                        .unwrap_or_else(|| "unnamed".to_string());

                                    match plan.try_add_dependency(parent_tid, child_tid) {
                                        Ok(()) => {} // Successfully linked
                                        Err(err) => {
                                            eprintln!(
                                                "Warning: Cannot link struct '{}' field '{}' -> '{}': {}",
                                                parent_name, field_name, child_name, err
                                            );
                                            if err.contains("cycle") {
                                                eprintln!("  This typically happens with recursive type definitions (e.g., Box<Self>, Rc<Self>)");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Link methods to their structs via impl blocks
    // ------------------------------------------------------------------
    eprintln!("\nAnalyzing impl blocks and methods...");
    for item in krate.index.values() {
        if item.crate_id != 0 {
            continue;
        }
        if let ItemEnum::Impl(imp) = &item.inner {
            if let Type::ResolvedPath(p) = &imp.for_ {
                if local_ids.contains(&p.id) {
                    if let Some(&parent_tid) = id_to_task.get(&p.id) {
                        let parent_path = path_map
                            .get(&p.id)
                            .map(|v| v.join("::"))
                            .unwrap_or_else(|| p.id.0.to_string());
                        for mid in &imp.items {
                            // Skip if this method was filtered out during creation
                            if !id_to_task.contains_key(mid) {
                                continue;
                            }

                            if let Some(&child_tid) = id_to_task.get(mid) {
                                let method_name = if let Some(method_item) = krate.index.get(mid) {
                                    if let Some(name) = &method_item.name {
                                        let task = plan.task_mut(child_tid);
                                        task.name = format!("{}::{}", parent_path, name);
                                        name.clone()
                                    } else {
                                        "unnamed".to_string()
                                    }
                                } else {
                                    "unnamed".to_string()
                                };

                                match plan.try_add_dependency(parent_tid, child_tid) {
                                    Ok(()) => {} // Successfully linked
                                    Err(err) => {
                                        eprintln!(
                                            "Warning: Cannot link impl '{}' -> method '{}': {}",
                                            parent_path, method_name, err
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Link trait methods to their trait definitions
    // ------------------------------------------------------------------
    eprintln!("\nAnalyzing trait methods...");
    for (id, item) in &krate.index {
        if item.crate_id != 0 {
            continue;
        }
        if let ItemEnum::Trait(tr) = &item.inner {
            if let Some(&parent_tid) = id_to_task.get(id) {
                let trait_path = path_map
                    .get(id)
                    .map(|v| v.join("::"))
                    .unwrap_or_else(|| item.name.clone().unwrap_or_default());
                for tid in &tr.items {
                    // Skip if this method was filtered out during creation
                    if !id_to_task.contains_key(tid) {
                        continue;
                    }

                    if let Some(&child_tid) = id_to_task.get(tid) {
                        let method_name = if let Some(child_item) = krate.index.get(tid) {
                            if let Some(name) = &child_item.name {
                                let task = plan.task_mut(child_tid);
                                task.name = format!("{}::{}", trait_path, name);
                                name.clone()
                            } else {
                                "unnamed".to_string()
                            }
                        } else {
                            "unnamed".to_string()
                        };

                        match plan.try_add_dependency(parent_tid, child_tid) {
                            Ok(()) => {} // Successfully linked
                            Err(err) => {
                                eprintln!(
                                    "Warning: Cannot link trait '{}' -> method '{}': {}",
                                    trait_path, method_name, err
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    eprintln!("\nGenerating output files...");
    fs::write(
        "plan-gen/bb_plan.plan.json",
        serde_json::to_string_pretty(&plan)?,
    )?;
    fs::write("plan-gen/bb_plan.plan.dot", plan.to_dot(cli.no_orphan))?;
    fs::write(
        "plan-gen/bb_plan.plan.graphml",
        plan.to_graphml(cli.no_orphan),
    )?;

    eprintln!("\nComplete!");
    eprintln!("Generated dependency graph with {} nodes", plan.len());
    if !cli.include_derives {
        eprintln!("(Filtered out common derive methods like clone, serialize, fmt, etc.)");
    }
    eprintln!("Output files:");
    eprintln!("  - bb_plan.plan.json");
    eprintln!("  - bb_plan.plan.dot");
    eprintln!("  - bb_plan.plan.graphml");

    if let Some(module) = cli.module {
        let mut prefixes = Vec::new();
        let module_with_lib = if module.ends_with("::lib") {
            module.clone()
        } else {
            format!("{}::lib", module)
        };
        if let Some(struc) = cli.struct_name {
            prefixes.push(format!("{}::{}", module, struc));
            prefixes.push(format!("{}::{}", module_with_lib, struc));
        } else {
            prefixes.push(module.clone());
            prefixes.push(module_with_lib.clone());
        }
        let dot = plan.to_dot_filtered(
            |task| prefixes.iter().any(|p| task.name.starts_with(p)),
            cli.no_orphan,
        );
        let base = prefixes
            .first()
            .cloned()
            .unwrap_or_else(|| module.clone())
            .replace("::", "_");
        let dot_path = format!("plan-gen/{}.dot", base);
        let png_path = format!("plan-gen/{}.png", base);
        fs::write(&dot_path, dot)?;
        let status = Command::new("dot")
            .args(["-Tpng", &dot_path, "-o", &png_path])
            .status()
            .context("running dot for PNG")?;
        if status.success() {
            println!("Wrote filtered PNG to {}", png_path);
        } else {
            println!("dot command failed; no PNG generated");
        }
    }

    if let Some(start_time) = start {
        let duration = start_time.elapsed();
        eprintln!("\nTotal execution time: {:?}", duration);
    }

    Ok(())
}

fn run_rustdoc() -> Result<()> {
    Command::new("cargo")
        .args([
            "+nightly",
            "rustdoc",
            "--lib",
            "-p",
            "bb_plan",
            "-Z",
            "unstable-options",
            "--",
            "-Z",
            "unstable-options",
            "--output-format",
            "json",
            "--document-private-items",
        ])
        .status()
        .context("running cargo rustdoc")?
        .success()
        .then_some(())
        .ok_or_else(|| anyhow::anyhow!("rustdoc failed"))
}

fn collect_local_ids_from_type(paths: &HashMap<Id, Vec<String>>, ty: &Type, out: &mut Vec<Id>) {
    use rustdoc_types::Type::*;
    match ty {
        ResolvedPath(p) => {
            if paths.contains_key(&p.id) {
                out.push(p.id);
            }
            if let Some(args) = &p.args {
                collect_local_ids_from_generic_args(paths, args, out);
            }
        }
        Tuple(ts) => {
            for t in ts {
                collect_local_ids_from_type(paths, t, out);
            }
        }
        Slice(t) => collect_local_ids_from_type(paths, t, out),
        Array { type_, .. } => collect_local_ids_from_type(paths, type_, out),
        BorrowedRef { type_, .. } => collect_local_ids_from_type(paths, type_, out),
        RawPointer { type_, .. } => collect_local_ids_from_type(paths, type_, out),
        QualifiedPath {
            self_type,
            trait_,
            args,
            ..
        } => {
            collect_local_ids_from_type(paths, self_type, out);
            if let Some(tr) = trait_ {
                if paths.contains_key(&tr.id) {
                    out.push(tr.id);
                }
                if let Some(a) = &tr.args {
                    collect_local_ids_from_generic_args(paths, a, out);
                }
            }
            if let Some(a) = args {
                collect_local_ids_from_generic_args(paths, a, out);
            }
        }
        FunctionPointer(fp) => {
            collect_local_ids_from_function_signature(paths, &fp.sig, out);
        }
        ImplTrait(bounds) => {
            for b in bounds {
                collect_local_ids_from_generic_bound(paths, b, out);
            }
        }
        DynTrait(dyn_trait) => {
            for t in &dyn_trait.traits {
                collect_local_ids_from_poly_trait(paths, t, out);
            }
        }
        _ => {}
    }
}

fn collect_local_ids_from_function_signature(
    paths: &HashMap<Id, Vec<String>>,
    sig: &rustdoc_types::FunctionSignature,
    out: &mut Vec<Id>,
) {
    for (_, t) in &sig.inputs {
        collect_local_ids_from_type(paths, t, out);
    }
    if let Some(t) = &sig.output {
        collect_local_ids_from_type(paths, t, out);
    }
}

fn collect_local_ids_from_generic_args(
    paths: &HashMap<Id, Vec<String>>,
    args: &rustdoc_types::GenericArgs,
    out: &mut Vec<Id>,
) {
    use rustdoc_types::{GenericArg, GenericArgs};
    match args {
        GenericArgs::AngleBracketed { args, constraints } => {
            for a in args {
                if let GenericArg::Type(t) = a {
                    collect_local_ids_from_type(paths, t, out);
                }
            }
            for c in constraints {
                if let Some(t) = &c.args {
                    collect_local_ids_from_generic_args(paths, t, out);
                }
                use rustdoc_types::AssocItemConstraintKind::*;
                match &c.binding {
                    Equality(term) => {
                        if let rustdoc_types::Term::Type(t) = term {
                            collect_local_ids_from_type(paths, t, out);
                        }
                    }
                    Constraint(bounds) => {
                        for b in bounds {
                            collect_local_ids_from_generic_bound(paths, b, out);
                        }
                    }
                }
            }
        }
        GenericArgs::Parenthesized { inputs, output } => {
            for t in inputs {
                collect_local_ids_from_type(paths, t, out);
            }
            if let Some(t) = output {
                collect_local_ids_from_type(paths, t, out);
            }
        }
        GenericArgs::ReturnTypeNotation => {}
    }
}

fn collect_local_ids_from_generic_bound(
    paths: &HashMap<Id, Vec<String>>,
    bound: &rustdoc_types::GenericBound,
    out: &mut Vec<Id>,
) {
    use rustdoc_types::GenericBound::*;
    match bound {
        TraitBound { trait_, .. } => {
            if paths.contains_key(&trait_.id) {
                out.push(trait_.id);
            }
            if let Some(args) = &trait_.args {
                collect_local_ids_from_generic_args(paths, args, out);
            }
        }
        _ => {}
    }
}

fn collect_local_ids_from_poly_trait(
    paths: &HashMap<Id, Vec<String>>,
    tr: &rustdoc_types::PolyTrait,
    out: &mut Vec<Id>,
) {
    if paths.contains_key(&tr.trait_.id) {
        out.push(tr.trait_.id);
    }
    if let Some(args) = &tr.trait_.args {
        collect_local_ids_from_generic_args(paths, args, out);
    }
    for gp in &tr.generic_params {
        if let rustdoc_types::GenericParamDefKind::Type { bounds, .. } = &gp.kind {
            for b in bounds {
                collect_local_ids_from_generic_bound(paths, b, out);
            }
        }
    }
}
