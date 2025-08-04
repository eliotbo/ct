use ct_core::config::Config;
use ct_core::transport::IpcClient;
use ct_protocol::{Request, Response, Command};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::path::PathBuf;
use uuid::Uuid;
use anyhow::{Context, Result};

pub struct Repl {
    config: Config,
    workspace_fingerprint: String,
    _workspace_root: PathBuf,
    current_path: String,
    client: Option<IpcClient>,
}

impl Repl {
    pub fn new(
        config: Config,
        workspace_fingerprint: String,
        workspace_root: PathBuf,
    ) -> Result<Self> {
        Ok(Self {
            config,
            workspace_fingerprint,
            _workspace_root: workspace_root,
            current_path: "crate".to_string(),
            client: None,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        // Try to connect to daemon
        match self.connect_daemon().await {
            Ok(_) => println!("Connected to ct-daemon"),
            Err(e) => {
                eprintln!("Warning: Could not connect to daemon: {}", e);
                eprintln!("Some commands may not work. Start ct-daemon first.");
            }
        }

        let mut rl = DefaultEditor::new()?;
        
        loop {
            let prompt = format!("(ct {})> ", self.current_path);
            let readline = rl.readline(&prompt);
            
            match readline {
                Ok(line) => {
                    rl.add_history_entry(line.as_str())?;
                    
                    if let Err(e) = self.handle_command(&line).await {
                        eprintln!("Error: {}", e);
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("^C");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    println!("^D");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            }
        }
        
        Ok(())
    }

    async fn connect_daemon(&mut self) -> Result<()> {
        let client = IpcClient::connect(&self.config, &self.workspace_fingerprint).await
            .context("Failed to connect to daemon")?;
        self.client = Some(client);
        Ok(())
    }

    async fn handle_command(&mut self, input: &str) -> Result<()> {
        let parts: Vec<&str> = input.trim().split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(());
        }
        
        match parts[0] {
            "help" | "?" => {
                self.print_help();
            }
            "quit" | "exit" | "q" => {
                std::process::exit(0);
            }
            "cd" => {
                if parts.len() < 2 {
                    println!("Usage: cd <path>");
                } else {
                    self.current_path = parts[1].to_string();
                }
            }
            "ls" => {
                let path = if parts.len() > 1 {
                    parts[1].to_string()
                } else {
                    self.current_path.clone()
                };
                
                let expansion = if parts.len() > 2 {
                    Some(parts[2..].join(""))
                } else {
                    None
                };
                
                self.send_ls_command(path, expansion).await?;
            }
            "doc" => {
                if parts.len() < 2 {
                    println!("Usage: doc <path>");
                } else {
                    let path = parts[1].to_string();
                    self.send_doc_command(path).await?;
                }
            }
            "find" => {
                if parts.len() < 2 {
                    println!("Usage: find <name>");
                } else {
                    let name = parts[1..].join(" ");
                    self.send_find_command(name).await?;
                }
            }
            "export" => {
                if parts.len() < 2 {
                    println!("Usage: export <path> [expansion]");
                } else {
                    let path = parts[1].to_string();
                    let expansion = if parts.len() > 2 {
                        Some(parts[2..].join(""))
                    } else {
                        None
                    };
                    self.send_export_command(path, expansion).await?;
                }
            }
            cmd if cmd.starts_with('!') => {
                // Shell escape
                let shell_cmd = &input[1..];
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(shell_cmd)
                    .output()?;
                
                print!("{}", String::from_utf8_lossy(&output.stdout));
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
            }
            _ => {
                println!("Unknown command: '{}'. Type 'help' for available commands.", parts[0]);
            }
        }
        
        Ok(())
    }

    fn print_help(&self) {
        println!("Available commands:");
        println!("  help, ?           - Show this help");
        println!("  cd <path>         - Change current path context");
        println!("  ls [path] [exp]   - List symbols (exp: >, >>, <, <<)");
        println!("  doc <path>        - Show documentation for symbol");
        println!("  find <name>       - Find symbols by name");
        println!("  export <path>     - Export symbol bundle");
        println!("  !<cmd>            - Execute shell command");
        println!("  quit, exit, q     - Exit REPL");
    }

    async fn send_command(&mut self, cmd: Command) -> Result<Response> {
        if let Some(client) = &mut self.client {
            let request = Request {
                cmd,
                request_id: Uuid::new_v4().to_string(),
                protocol_version: ct_protocol::PROTOCOL_VERSION,
            };
            
            client.send_request(request).await
                .context("Failed to send request")
        } else {
            Err(anyhow::anyhow!("Not connected to daemon"))
        }
    }

    async fn send_ls_command(&mut self, path: String, expansion: Option<String>) -> Result<()> {
        let cmd = Command::Ls {
            path,
            expansion,
            impl_parents: false,
            include_docs: false,
            vis: None,
            unimplemented: None,
            todo: None,
        };
        
        let response = self.send_command(cmd).await?;
        self.print_response(response);
        Ok(())
    }

    async fn send_doc_command(&mut self, path: String) -> Result<()> {
        let cmd = Command::Doc {
            path,
            include_docs: true,
            vis: None,
            unimplemented: None,
            todo: None,
        };
        
        let response = self.send_command(cmd).await?;
        self.print_response(response);
        Ok(())
    }

    async fn send_find_command(&mut self, name: String) -> Result<()> {
        let cmd = Command::Find {
            name: Some(name),
            path: None,
            kind: None,
            vis: None,
            unimplemented: None,
            todo: None,
            all: None,
        };
        
        let response = self.send_command(cmd).await?;
        self.print_response(response);
        Ok(())
    }

    async fn send_export_command(&mut self, path: String, expansion: Option<String>) -> Result<()> {
        let cmd = Command::Export {
            path,
            bundle: true,
            expansion,
            include_docs: true,
            impl_parents: false,
            vis: None,
            unimplemented: None,
            todo: None,
            with_source: false,
        };
        
        let response = self.send_command(cmd).await?;
        self.print_response(response);
        Ok(())
    }

    fn print_response(&self, response: Response) {
        match response {
            Response::Success(env) => {
                println!("{}", serde_json::to_string_pretty(&env.data).unwrap());
            }
            Response::Decision(env) => {
                println!("Decision required: {}", env.decision_required.reason);
                println!("Content length: {} bytes", env.decision_required.content_len);
                println!("Options: {:?}", env.decision_required.options);
            }
            Response::Error(env) => {
                eprintln!("Error: {}", env.err);
            }
        }
    }
}