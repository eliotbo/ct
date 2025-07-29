use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use crate::Result;

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
}

impl FileWatcher {
    pub fn new(_workspace_root: &Path, debounce_ms: u64) -> Result<Self> {
        let (tx, rx) = channel();
        
        let config = Config::default()
            .with_poll_interval(Duration::from_millis(debounce_ms));
        
        let watcher = RecommendedWatcher::new(
            move |res| {
                if let Err(e) = tx.send(res) {
                    error!("Failed to send watch event: {}", e);
                }
            },
            config,
        )?;
        
        Ok(Self { watcher, rx })
    }

    pub fn watch(&mut self, path: &Path) -> Result<()> {
        info!("Starting file watcher for {:?}", path);
        self.watcher.watch(path, RecursiveMode::Recursive)?;
        Ok(())
    }

    pub fn collect_changes(&mut self, debounce_ms: u64) -> Vec<PathBuf> {
        let mut changed_files = Vec::new();
        let start = std::time::Instant::now();
        let debounce_duration = Duration::from_millis(debounce_ms);
        
        // Collect all events within debounce window
        while start.elapsed() < debounce_duration {
            match self.rx.try_recv() {
                Ok(Ok(event)) => {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            for path in event.paths {
                                if is_rust_file(&path) && !is_ignored(&path) {
                                    debug!("File changed: {:?}", path);
                                    changed_files.push(path);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Err(e)) => {
                    error!("Watch error: {}", e);
                }
                Err(_) => {
                    // No more events, wait a bit
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        
        // Deduplicate
        changed_files.sort();
        changed_files.dedup();
        
        if !changed_files.is_empty() {
            info!("Collected {} changed files after debounce", changed_files.len());
        }
        
        changed_files
    }
}

fn is_rust_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext == "rs")
        .unwrap_or(false)
}

fn is_ignored(path: &Path) -> bool {
    // Ignore target directory and hidden files
    path.components().any(|c| {
        c.as_os_str().to_str()
            .map(|s| s == "target" || s.starts_with('.'))
            .unwrap_or(false)
    })
}

pub struct WatcherHandle {
    tx: mpsc::Sender<WatcherCommand>,
}

pub enum WatcherCommand {
    GetChanges,
    Stop,
}

impl WatcherHandle {
    pub async fn request_changes(&self) -> Result<Vec<PathBuf>> {
        // Stub for now
        Ok(vec![])
    }
    
    pub async fn stop(&self) -> Result<()> {
        self.tx.send(WatcherCommand::Stop).await
            .map_err(|e| crate::IndexError::IndexingFailed(e.to_string()))?;
        Ok(())
    }
}

pub async fn spawn_watcher(
    workspace_root: PathBuf,
    debounce_ms: u64,
) -> Result<WatcherHandle> {
    let (tx, mut rx) = mpsc::channel(100);
    
    tokio::spawn(async move {
        let mut watcher = match FileWatcher::new(&workspace_root, debounce_ms) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create file watcher: {}", e);
                return;
            }
        };
        
        if let Err(e) = watcher.watch(&workspace_root) {
            error!("Failed to start watching: {}", e);
            return;
        }
        
        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        WatcherCommand::GetChanges => {
                            let _changes = watcher.collect_changes(debounce_ms);
                            // In real implementation, would send changes back
                        }
                        WatcherCommand::Stop => {
                            info!("Stopping file watcher");
                            break;
                        }
                    }
                }
                else => {
                    break;
                }
            }
        }
    });
    
    Ok(WatcherHandle { tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_rust_file() {
        assert!(is_rust_file(Path::new("main.rs")));
        assert!(is_rust_file(Path::new("src/lib.rs")));
        assert!(!is_rust_file(Path::new("Cargo.toml")));
        assert!(!is_rust_file(Path::new("README.md")));
    }

    #[test]
    fn test_is_ignored() {
        assert!(is_ignored(Path::new("target/debug/main")));
        assert!(is_ignored(Path::new(".git/config")));
        assert!(!is_ignored(Path::new("src/main.rs")));
    }
}