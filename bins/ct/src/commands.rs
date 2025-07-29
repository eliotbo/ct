use crate::client::CtClient;
use crate::OutputFormat;
use ct_core::utils::*;
use ct_protocol::{Command, Response, ErrorCode};
use anyhow::Result;

pub async fn find(
    query: String,
    kind: Option<String>,
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
    
    let cmd = Command::Find {
        name: Some(query.clone()),
        path: None,
        kind,
        vis,
        unimplemented: if unimplemented { Some(true) } else { None },
        todo: if todo { Some(true) } else { None },
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
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
    docs: bool,
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
        include_docs: docs,
        vis,
        unimplemented: if unimplemented { Some(true) } else { None },
        todo: if todo { Some(true) } else { None },
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

pub async fn export(
    path: String,
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
    
    let cmd = Command::Export {
        path,
        bundle,
        expansion: if expansion.is_empty() { None } else { Some(expansion) },
        include_docs,
        impl_parents,
        vis,
        unimplemented: if unimplemented { Some(true) } else { None },
        todo: if todo { Some(true) } else { None },
        with_source,
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

pub async fn reindex(
    features: Vec<String>,
    target: Option<String>,
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

pub async fn diag(
    format: OutputFormat,
    pretty: bool,
) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    let cmd = Command::Diag;
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

pub async fn bench(
    queries: u32,
    warmup: u32,
    duration: u32,
    format: OutputFormat,
    pretty: bool,
) -> Result<u8> {
    let mut client = match CtClient::connect().await {
        Ok(c) => c,
        Err(_) => return Ok(EXIT_DAEMON_UNAVAILABLE),
    };
    
    let cmd = Command::Bench {
        queries,
        warmup,
        duration,
    };
    
    let response = client.send_command(cmd).await?;
    print_response(response, format, pretty)
}

fn print_response(response: Response, format: OutputFormat, pretty: bool) -> Result<u8> {
    match response {
        Response::Success(env) => {
            match format {
                OutputFormat::Json => {
                    if pretty {
                        println!("{}", serde_json::to_string_pretty(&env)?);
                    } else {
                        println!("{}", serde_json::to_string(&env)?);
                    }
                }
                OutputFormat::Pretty => {
                    // In a real implementation, would format nicely
                    println!("{}", serde_json::to_string_pretty(&env.data)?);
                }
            }
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