mod repl;

use clap::Parser;
use ct_core::{config::Config, compute_workspace_fingerprint, utils::find_workspace_root};
use std::path::PathBuf;
use anyhow::Result;

#[derive(Parser, Debug)]
#[command(name = "ctrepl")]
#[command(about = "Interactive REPL for ct", version)]
struct Args {
    /// Path to workspace
    #[arg(long = "idx", value_name = "PATH")]
    workspace: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    let workspace_root = if let Some(path) = args.workspace {
        path.canonicalize()?
    } else {
        find_workspace_root(&std::env::current_dir()?)?
    };
    
    let config = Config::load()?;
    let workspace_fingerprint = compute_workspace_fingerprint(&workspace_root);
    
    println!("ct REPL - Interactive symbol explorer");
    println!("Type 'help' for commands, 'quit' to exit\n");
    
    let mut repl = repl::Repl::new(config, workspace_fingerprint, workspace_root)?;
    repl.run().await?;
    
    println!("\nGoodbye!");
    Ok(())
}