mod client;
mod commands;

use clap::{Parser, Subcommand};
use ct_core::utils::EXIT_INVALID_ARGS;

#[derive(Parser)]
#[command(name = "ct")]
#[command(about = "Symbol-centric code explorer for Rust", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Output format
    #[arg(long, global = true, value_enum, default_value = "json")]
    format: OutputFormat,
    
    /// Pretty-print output
    #[arg(long, global = true)]
    pretty: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum OutputFormat {
    Json,
    Pretty,
}

#[derive(Subcommand)]
enum Commands {
    /// Find symbols by name or path
    Find {
        /// Name or path to search for
        query: String,
        
        /// Symbol kind filter
        #[arg(long)]
        kind: Option<String>,
        
        /// Visibility filter (public, private, all)
        #[arg(long, value_name = "VIS")]
        vis: Option<String>,
        
        /// Show only unimplemented symbols
        #[arg(short = 'u', long)]
        unimplemented: bool,
        
        /// Show only todo symbols
        #[arg(short = 't', long)]
        todo: bool,
    },
    
    /// Show documentation for a symbol
    Doc {
        /// Path to the symbol
        path: String,
        
        /// Include documentation
        #[arg(short = 'd', long)]
        docs: bool,
        
        /// Include documentation for all expanded items
        #[arg(long, value_name = "all")]
        docs_all: bool,
        
        /// Visibility filter
        #[arg(long)]
        vis: Option<String>,
        
        /// Show only unimplemented symbols
        #[arg(short = 'u', long)]
        unimplemented: bool,
        
        /// Show only todo symbols
        #[arg(short = 't', long)]
        todo: bool,
    },
    
    /// List symbols with expansion
    Ls {
        /// Path to list
        path: String,
        
        /// Expansion operators (e.g., ">", ">>", "<", "<<")
        #[arg(value_name = "EXPANSION")]
        expansion: Vec<String>,
        
        /// Enable impl-parents traversal
        #[arg(long)]
        impl_parents: bool,
        
        /// Include documentation
        #[arg(short = 'd', long)]
        docs: bool,
        
        /// Visibility filter
        #[arg(long)]
        vis: Option<String>,
        
        /// Show only unimplemented symbols
        #[arg(short = 'u', long)]
        unimplemented: bool,
        
        /// Show only todo symbols
        #[arg(short = 't', long)]
        todo: bool,
        
        /// Maximum context size override
        #[arg(long)]
        max_size: Option<usize>,
    },
    
    /// Export symbol bundle
    Export {
        /// Path to export
        path: String,
        
        /// Export as bundle
        #[arg(long)]
        bundle: bool,
        
        /// Include documentation
        #[arg(short = 'd', long)]
        docs: bool,
        
        /// Include documentation for all expanded items
        #[arg(long, value_name = "all")]
        docs_all: bool,
        
        /// Expansion operators
        #[arg(value_name = "EXPANSION")]
        expansion: Vec<String>,
        
        /// Enable impl-parents traversal
        #[arg(long)]
        impl_parents: bool,
        
        /// Visibility filter
        #[arg(long)]
        vis: Option<String>,
        
        /// Show only unimplemented symbols
        #[arg(short = 'u', long)]
        unimplemented: bool,
        
        /// Show only todo symbols
        #[arg(short = 't', long)]
        todo: bool,
        
        /// Include source snippets
        #[arg(long)]
        with_source: bool,
        
        /// Maximum context size override
        #[arg(long)]
        max_size: Option<usize>,
    },
    
    /// Trigger reindexing
    Reindex {
        /// Features to enable
        #[arg(long)]
        features: Vec<String>,
        
        /// Target triple
        #[arg(long)]
        target: Option<String>,
    },
    
    /// Show implementation status
    Status {
        /// Visibility filter
        #[arg(long)]
        vis: Option<String>,
        
        /// Show only unimplemented symbols
        #[arg(short = 'u', long)]
        unimplemented: bool,
        
        /// Show only todo symbols
        #[arg(short = 't', long)]
        todo: bool,
    },
    
    /// Show diagnostics
    Diag,
    
    /// Run benchmarks
    Bench {
        /// Number of queries
        #[arg(long, default_value = "200")]
        queries: u32,
        
        /// Warmup duration in milliseconds
        #[arg(long, default_value = "100")]
        warmup: u32,
        
        /// Benchmark duration in seconds
        #[arg(long, default_value = "5")]
        duration: u32,
    },
    
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    
    let exit_code = match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {}", e);
            EXIT_INVALID_ARGS
        }
    };
    
    std::process::exit(exit_code as i32);
}

async fn run(cli: Cli) -> anyhow::Result<u8> {
    match cli.command {
        Commands::Find { query, kind, vis, unimplemented, todo } => {
            commands::find(query, kind, vis, unimplemented, todo, cli.format, cli.pretty).await
        }
        Commands::Doc { path, docs, docs_all, vis, unimplemented, todo } => {
            let include_docs = docs || docs_all;
            commands::doc(path, include_docs, vis, unimplemented, todo, cli.format, cli.pretty).await
        }
        Commands::Ls { path, expansion, impl_parents, docs, vis, unimplemented, todo, max_size } => {
            let expansion_str = expansion.join("");
            commands::ls(path, expansion_str, impl_parents, docs, vis, unimplemented, todo, max_size, cli.format, cli.pretty).await
        }
        Commands::Export { path, bundle, docs, docs_all, expansion, impl_parents, vis, unimplemented, todo, with_source, max_size } => {
            let include_docs = docs || docs_all;
            let expansion_str = expansion.join("");
            commands::export(path, bundle, expansion_str, include_docs, impl_parents, vis, unimplemented, todo, with_source, max_size, cli.format, cli.pretty).await
        }
        Commands::Reindex { features, target } => {
            commands::reindex(features, target, cli.format, cli.pretty).await
        }
        Commands::Status { vis, unimplemented, todo } => {
            commands::status(vis, unimplemented, todo, cli.format, cli.pretty).await
        }
        Commands::Diag => {
            commands::diag(cli.format, cli.pretty).await
        }
        Commands::Bench { queries, warmup, duration } => {
            commands::bench(queries, warmup, duration, cli.format, cli.pretty).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_args() {
        Cli::command().debug_assert();
    }
}