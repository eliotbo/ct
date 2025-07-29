use ct_core::config::{Config, Transport};
use ct_protocol::{Request, Response, ErrorCode, deserialize_message, serialize_message};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info};
use crate::state::DaemonState;

#[cfg(windows)]
use tokio::net::windows::named_pipe::{ServerOptions, NamedPipeServer};

pub struct ServerHandle {
    shutdown_tx: mpsc::Sender<()>,
}

impl ServerHandle {
    pub async fn shutdown(self) -> anyhow::Result<()> {
        self.shutdown_tx.send(()).await?;
        Ok(())
    }
}

pub async fn start_server(
    config: Config,
    workspace_fingerprint: String,
) -> anyhow::Result<ServerHandle> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
    
    let state = Arc::new(Mutex::new(DaemonState::new(
        config.clone(),
        workspace_fingerprint.clone(),
    )));
    
    let transport = config.get_effective_transport();
    
    match transport {
        #[cfg(unix)]
        Transport::Unix => {
            let socket_path = config.get_socket_path(&workspace_fingerprint);
            
            // Remove existing socket if it exists
            if std::path::Path::new(&socket_path).exists() {
                std::fs::remove_file(&socket_path)?;
            }
            
            let listener = UnixListener::bind(&socket_path)?;
            info!("IPC server listening on Unix socket: {}", socket_path);
            
            tokio::spawn(async move {
                unix_server_loop(listener, state, shutdown_rx).await;
            });
        }
        
        #[cfg(windows)]
        Transport::Pipe => {
            let pipe_name = config.get_pipe_name(&workspace_fingerprint);
            info!("IPC server listening on named pipe: {}", pipe_name);
            
            tokio::spawn(async move {
                pipe_server_loop(pipe_name, state, shutdown_rx).await;
            });
        }
        
        Transport::Tcp => {
            let listener = TcpListener::bind(&config.tcp_addr).await?;
            info!("IPC server listening on TCP: {}", config.tcp_addr);
            
            tokio::spawn(async move {
                tcp_server_loop(listener, state, shutdown_rx).await;
            });
        }
        
        _ => {
            return Err(anyhow::anyhow!("Unsupported transport: {:?}", transport));
        }
    }
    
    Ok(ServerHandle { shutdown_tx })
}

#[cfg(unix)]
async fn unix_server_loop(
    listener: UnixListener,
    state: Arc<Mutex<DaemonState>>,
    mut shutdown_rx: mpsc::Receiver<()>,
) {
    loop {
        tokio::select! {
            Ok((stream, _)) = listener.accept() => {
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        error!("Error handling connection: {}", e);
                    }
                });
            }
            _ = shutdown_rx.recv() => {
                info!("Unix server shutting down");
                break;
            }
        }
    }
}

#[cfg(windows)]
async fn pipe_server_loop(
    pipe_name: String,
    state: Arc<Mutex<DaemonState>>,
    mut shutdown_rx: mpsc::Receiver<()>,
) {
    loop {
        let server = match ServerOptions::new()
            .first_pipe_instance(false)
            .create(&pipe_name)
        {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create named pipe: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
        };
        
        tokio::select! {
            _ = server.connect() => {
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(server, state).await {
                        error!("Error handling connection: {}", e);
                    }
                });
            }
            _ = shutdown_rx.recv() => {
                info!("Pipe server shutting down");
                break;
            }
        }
    }
}

async fn tcp_server_loop(
    listener: TcpListener,
    state: Arc<Mutex<DaemonState>>,
    mut shutdown_rx: mpsc::Receiver<()>,
) {
    loop {
        tokio::select! {
            Ok((stream, addr)) = listener.accept() => {
                debug!("New TCP connection from: {}", addr);
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        error!("Error handling connection: {}", e);
                    }
                });
            }
            _ = shutdown_rx.recv() => {
                info!("TCP server shutting down");
                break;
            }
        }
    }
}

async fn handle_connection<S>(
    stream: S,
    state: Arc<Mutex<DaemonState>>,
) -> anyhow::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        
        if n == 0 {
            // Client disconnected
            break;
        }
        
        let request: Request = match deserialize_message(line.trim()) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse request: {}", e);
                let response = Response::error(
                    "unknown".to_string(),
                    format!("Invalid request: {}", e),
                    ErrorCode::ProtocolError,
                );
                let msg = serialize_message(&response)?;
                writer.write_all(format!("{}\n", msg).as_bytes()).await?;
                writer.flush().await?;
                continue;
            }
        };
        
        debug!("Received request: {:?}", request.cmd);
        
        let response = {
            let mut state = state.lock().await;
            state.handle_request(request).await
        };
        
        let msg = serialize_message(&response)?;
        writer.write_all(format!("{}\n", msg).as_bytes()).await?;
        writer.flush().await?;
    }
    
    Ok(())
}