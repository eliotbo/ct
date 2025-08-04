use crate::client::CtClient;
use crate::OutputFormat;
use crate::DaemonCommand;
use ct_core::utils::*;
use ct_protocol::{Command, Response, ErrorCode};
use anyhow::Result;
use std::process::Command as ProcessCommand;
use ct_core::config::Config;
use ct_core::compute_workspace_fingerprint;
use serde_json::json;

pub async fn find(
    query: String,
    kind: Option<String>,
    vis: Option<String>,
    unimplemented: bool,
    todo: bool,
    all: bool,
    format: OutputFormat,
    pretty: bool,
) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    let cmd = Command::Find {
        name: Some(query.clone()),
        path: None,
        kind,
        vis,
        unimplemented: if unimplemented { Some(true) } else { None },
        todo: if todo { Some(true) } else { None },
        all: if all { Some(true) } else { None },
    };
    
    let response = client.send_command(cmd).await?;
    print_find_response(response, format, pretty, all)
}

pub async fn doc(
    path: String,
    include_docs: bool,
    vis: Option<String>,
    unimplemented: bool,
    todo: bool,
    format: OutputFormat,
    pretty: bool,
) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    let cmd = Command::Doc {
        path,
        include_docs,
        vis,
        unimplemented: if unimplemented { Some(true) } else { None },
        todo: if todo { Some(true) } else { None },
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

pub async fn ls(
    path: String,
    expansion: String,
    impl_parents: bool,
    include_docs: bool,
    vis: Option<String>,
    unimplemented: bool,
    todo: bool,
    _max_size: Option<usize>,
    format: OutputFormat,
    pretty: bool,
) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    let cmd = Command::Ls {
        path,
        expansion: if expansion.is_empty() { None } else { Some(expansion) },
        impl_parents,
        include_docs,
        vis,
        unimplemented: if unimplemented { Some(true) } else { None },
        todo: if todo { Some(true) } else { None },
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

pub async fn export(
    path: Vec<String>,
    bundle: bool,
    expansion: String,
    include_docs: bool,
    impl_parents: bool,
    vis: Option<String>,
    unimplemented: bool,
    todo: bool,
    with_source: bool,
    _max_size: Option<usize>,
    format: OutputFormat,
    pretty: bool,
) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    // For now, just use the first path - we may need to update the protocol to support multiple paths
    let single_path = path.into_iter().next().unwrap_or_default();
    
    let cmd = Command::Export {
        path: single_path,
        bundle,
        expansion: if expansion.is_empty() { None } else { Some(expansion) },
        include_docs,
        vis,
        unimplemented: if unimplemented { Some(true) } else { None },
        todo: if todo { Some(true) } else { None },
        impl_parents,
        with_source,
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

pub async fn reindex(
    features: Vec<String>,
    target: Option<String>,
    module: Option<String>,
    struct_name: Option<String>,
    include_derives: bool,
    format: OutputFormat,
    pretty: bool,
) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    let cmd = Command::Reindex {
        features: if features.is_empty() { None } else { Some(features) },
        target,
        module,
        struct_name,
        include_derives,
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

pub async fn status(
    vis: Option<String>,
    unimplemented: bool,
    todo: bool,
    format: OutputFormat,
    pretty: bool,
) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    let cmd = Command::Status {
        vis,
        unimplemented: if unimplemented { Some(true) } else { None },
        todo: if todo { Some(true) } else { None },
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

pub async fn diag(format: OutputFormat, pretty: bool) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    let response = client.send_command(Command::Diag).await?;
    print_response(response, format, pretty)
}

pub async fn bench(
    _queries: u32,
    _warmup: u32,
    _duration: u32,
    _format: OutputFormat,
    _pretty: bool,
) -> Result<u8> {
    let _client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    // TODO: Implement benchmarking
    eprintln!("Benchmarking not yet implemented");
    Ok(EXIT_OK)
}

fn print_find_response(
    response: Response,
    format: OutputFormat,
    pretty: bool,
    all: bool,
) -> Result<u8> {
    match response {
        Response::Success(env) => {
            match format {
                OutputFormat::Json => {
                    if all {
                        // Show full symbol data
                        let output = if pretty {
                            serde_json::to_string_pretty(&env.data)?
                        } else {
                            serde_json::to_string(&env.data)?
                        };
                        println!("{}", output);
                    } else {
                        // Show only paths and spans
                        if let Some(symbols) = env.data.get("symbols").and_then(|s| s.as_array()) {
                            let simplified: Vec<_> = symbols.iter()
                                .map(|s| json!({
                                    "path": s.get("path"),
                                    "span": s.get("span"),
                                }))
                                .collect();
                            
                            let output = if pretty {
                                serde_json::to_string_pretty(&simplified)?
                            } else {
                                serde_json::to_string(&simplified)?
                            };
                            println!("{}", output);
                        } else {
                            println!("{}", if pretty {
                                serde_json::to_string_pretty(&env.data)?
                            } else {
                                serde_json::to_string(&env.data)?
                            });
                        }
                    }
                }
                OutputFormat::Pretty => {
                    if let Some(symbols) = env.data.get("symbols").and_then(|s| s.as_array()) {
                        for symbol in symbols {
                            if let Some(path) = symbol.get("path").and_then(|p| p.as_str()) {
                                println!("{}", path);
                                if let Some(span) = symbol.get("span").and_then(|s| s.as_object()) {
                                    if let (Some(file), Some(line), Some(col)) = (
                                        span.get("file").and_then(|f| f.as_str()),
                                        span.get("line").and_then(|l| l.as_u64()),
                                        span.get("col").and_then(|c| c.as_u64())
                                    ) {
                                        println!("  at {}:{}:{}", file, line, col);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(EXIT_OK)
        }
        _ => print_response(response, format, pretty),
    }
}

fn print_response(response: Response, _format: OutputFormat, pretty: bool) -> Result<u8> {
    match response {
        Response::Success(env) => {
            let output = if pretty {
                serde_json::to_string_pretty(&env.data)?
            } else {
                serde_json::to_string(&env.data)?
            };
            println!("{}", output);
            Ok(EXIT_OK)
        }
        Response::Decision(env) => {
            eprintln!("Decision required: {}", env.decision_required.reason);
            eprintln!("Content length: {} bytes", env.decision_required.content_len);
            eprintln!("Options: {:?}", env.decision_required.options);
            Ok(EXIT_OVER_MAX)
        }
        Response::Error(env) => {
            eprintln!("Error: {}", env.err);
            match env.err_code {
                ErrorCode::InvalidArg => Ok(EXIT_INVALID_ARGS),
                ErrorCode::DaemonUnavailable => Ok(EXIT_DAEMON_UNAVAILABLE),
                ErrorCode::IndexMismatch => Ok(EXIT_INDEX_MISMATCH),
                _ => Ok(EXIT_INTERNAL_ERROR),
            }
        }
    }
}

pub async fn daemon(command: DaemonCommand) -> Result<u8> {
    match command {
        DaemonCommand::Start { idx, clean, transport } => {
            daemon_start(idx, clean, transport).await
        }
        DaemonCommand::Stop => {
            daemon_stop().await
        }
        DaemonCommand::Restart { idx, transport } => {
            daemon_restart(idx, transport).await
        }
        DaemonCommand::Status => {
            daemon_status().await
        }
    }
}

async fn daemon_start(idx: String, clean: bool, transport: String) -> Result<u8> {
    let config = Config::load()?;
    
    // Get workspace fingerprint
    let workspace_root = std::path::Path::new(&idx).canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&idx));
    let workspace_fingerprint = compute_workspace_fingerprint(&workspace_root);
    
    println!("Starting daemon for workspace: {}", workspace_root.display());
    
    // Clean cache if requested
    if clean {
        let cache_dir = config.get_cache_dir(&workspace_fingerprint);
        if cache_dir.exists() {
            println!("Cleaning cache directory: {}", cache_dir.display());
            std::fs::remove_dir_all(&cache_dir)?;
        }
    }
    
    // Remove existing socket file if it exists
    #[cfg(unix)]
    {
        let socket_path = config.get_socket_path(&workspace_fingerprint);
        if std::path::Path::new(&socket_path).exists() {
            std::fs::remove_file(&socket_path)?;
        }
    }
    
    // Check if daemon is already running
    if let Ok(mut client) = CtClient::connect().await {
        match client.send_command(Command::Diag).await {
            Ok(Response::Success(_)) => {
                eprintln!("Daemon is already running");
                return Ok(EXIT_DAEMON_ALREADY_RUNNING);
            }
            _ => {}
        }
    }
    
    // Start the daemon
    // Find ct-daemon in PATH or same directory as ct
    let daemon_path = if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap();
        let daemon = dir.join("ct-daemon");
        if daemon.exists() {
            daemon
        } else {
            // Fall back to PATH
            std::path::PathBuf::from("ct-daemon")
        }
    } else {
        std::path::PathBuf::from("ct-daemon")
    };
    
    let mut cmd = ProcessCommand::new(daemon_path);
    cmd.arg("--idx").arg(&workspace_root);
    cmd.arg("--transport").arg(&transport);
    
    if clean {
        cmd.arg("--clean");
    }
    
    // Run in background
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    cmd.stdin(std::process::Stdio::null());
    
    let child = cmd.spawn()?;
    println!("Started ct-daemon with PID: {}", child.id());
    
    // Wait a bit for the daemon to start
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    
    // Verify it started - try multiple times
    for attempt in 0..5 {
        if attempt > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }
        
        match CtClient::connect().await {
            Ok(mut client) => {
                match client.send_command(Command::Diag).await {
                    Ok(Response::Success(_)) => {
                        println!("Daemon started successfully");
                        return Ok(EXIT_OK);
                    }
                    Err(e) => {
                        eprintln!("Failed to send command to daemon: {}", e);
                    }
                    _ => {
                        eprintln!("Unexpected response from daemon");
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to connect to daemon (attempt {}): {}", attempt + 1, e);
            }
        }
    }
    
    eprintln!("Failed to verify daemon startup after 5 attempts");
    eprintln!("The daemon may still be running. Try 'ct daemon status' to check.");
    Ok(EXIT_OK)  // Return OK since the daemon process started
}

async fn daemon_stop() -> Result<u8> {
    let _config = Config::load()?;
    
    // Try to connect to daemon
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => {
            println!("Daemon is not running");
            return Ok(EXIT_OK);
        }
    };
    
    // Send shutdown signal (we'll use a diagnostic command and then kill the process)
    let _response = client.send_command(Command::Diag).await?;
    
    // Get PID from process list
    #[cfg(unix)]
    {
        let output = ProcessCommand::new("pgrep")
            .arg("-f")
            .arg("ct-daemon")
            .output()?;
            
        if output.status.success() {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid in pids.lines() {
                if let Ok(pid_num) = pid.trim().parse::<u32>() {
                    ProcessCommand::new("kill")
                        .arg(pid.trim())
                        .output()?;
                    println!("Stopped ct-daemon (PID: {})", pid_num);
                }
            }
        }
    }
    
    #[cfg(windows)]
    {
        ProcessCommand::new("taskkill")
            .arg("/F")
            .arg("/IM")
            .arg("ct-daemon.exe")
            .output()?;
        println!("Stopped ct-daemon");
    }
    
    Ok(EXIT_OK)
}

async fn daemon_restart(idx: String, transport: String) -> Result<u8> {
    println!("Stopping daemon...");
    daemon_stop().await?;
    
    // Wait a bit for cleanup
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    println!("Starting daemon with clean cache...");
    daemon_start(idx, true, transport).await
}

async fn daemon_status() -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => {
            println!("Daemon is not running");
            return Ok(EXIT_DAEMON_UNAVAILABLE);
        }
    };
    
    let response = client.send_command(Command::Diag).await?;
    
    match response {
        Response::Success(env) => {
            println!("Daemon is running");
            if let Some(version) = env.data.get("version").and_then(|v| v.as_str()) {
                println!("Version: {}", version);
            }
            if let Some(workspace) = env.data.get("workspace_root").and_then(|w| w.as_str()) {
                println!("Workspace: {}", workspace);
            }
            if let Some(timestamp) = env.data.get("index_timestamp").and_then(|t| t.as_str()) {
                println!("Index timestamp: {}", timestamp);
            }
            if let Some(symbols) = env.data.get("num_symbols").and_then(|s| s.as_u64()) {
                println!("Symbols: {}", symbols);
            }
            if let Some(crates) = env.data.get("num_crates").and_then(|c| c.as_u64()) {
                println!("Crates: {}", crates);
            }
            Ok(EXIT_OK)
        }
        _ => {
            println!("Daemon status unknown");
            Ok(EXIT_INTERNAL_ERROR)
        }
    }
}