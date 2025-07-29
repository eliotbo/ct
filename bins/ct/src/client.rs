use ct_core::{config::Config, compute_workspace_fingerprint, utils::find_workspace_root};
use ct_core::transport::IpcClient;
use ct_protocol::{Request, Response, Command};
use std::path::Path;
use uuid::Uuid;
use anyhow::{Context, Result};

pub struct CtClient {
    client: IpcClient,
}

impl CtClient {
    pub async fn connect() -> Result<Self> {
        let config = Config::load()?;
        let workspace_root = find_workspace_root(&std::env::current_dir()?)?;
        let workspace_fingerprint = compute_workspace_fingerprint(&workspace_root);
        
        // Try to connect to daemon
        match IpcClient::connect(&config, &workspace_fingerprint).await {
            Ok(client) => Ok(Self { client }),
            Err(_) if config.autostart => {
                // Try to start daemon
                Self::start_daemon(&workspace_root).await?;
                
                // Wait a bit for daemon to start
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                
                // Try connecting again
                let client = IpcClient::connect(&config, &workspace_fingerprint).await
                    .context("Failed to connect to daemon after autostart")?;
                
                Ok(Self { client })
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn start_daemon(workspace_root: &Path) -> Result<()> {
        use std::process::Command;
        
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
        
        Command::new(daemon_path)
            .arg("--idx")
            .arg(workspace_root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to start ct-daemon")?;
        
        Ok(())
    }

    pub async fn send_command(&mut self, cmd: Command) -> Result<Response> {
        let request = Request {
            cmd,
            request_id: Uuid::new_v4().to_string(),
            protocol_version: ct_protocol::PROTOCOL_VERSION,
        };
        
        self.client.send_request(request).await
            .context("Failed to send request to daemon")
    }
}

