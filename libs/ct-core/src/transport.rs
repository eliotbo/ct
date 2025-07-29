use crate::config::{Config, Transport as TransportType};
use crate::{CoreError, Result};
use ct_protocol::{Request, Response, serialize_message, deserialize_message};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[cfg(unix)]
use tokio::net::UnixStream;
#[cfg(windows)]
use tokio::net::windows::named_pipe::ClientOptions;

pub enum TransportStream {
    #[cfg(unix)]
    Unix(UnixStream),
    #[cfg(windows)]
    Pipe(tokio::net::windows::named_pipe::NamedPipeClient),
    Tcp(tokio::net::TcpStream),
}

impl TransportStream {
    pub async fn connect(config: &Config, workspace_fingerprint: &str) -> Result<Self> {
        match config.get_effective_transport() {
            #[cfg(unix)]
            TransportType::Unix => {
                let path = config.get_socket_path(workspace_fingerprint);
                let stream = UnixStream::connect(&path).await
                    .map_err(|e| CoreError::Io(e))?;
                Ok(TransportStream::Unix(stream))
            }
            #[cfg(windows)]
            TransportType::Pipe => {
                let pipe_name = config.get_pipe_name(workspace_fingerprint);
                let client = ClientOptions::new()
                    .open(&pipe_name)
                    .map_err(|e| CoreError::Io(e))?;
                Ok(TransportStream::Pipe(client))
            }
            TransportType::Tcp => {
                let stream = tokio::net::TcpStream::connect(&config.tcp_addr).await
                    .map_err(|e| CoreError::Io(e))?;
                Ok(TransportStream::Tcp(stream))
            }
            _ => Err(CoreError::Config("Unsupported transport".to_string())),
        }
    }

    pub async fn send_request(&mut self, request: &Request) -> Result<()> {
        let msg = serialize_message(request)
            .map_err(|e| CoreError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        let msg = format!("{}\n", msg);
        
        match self {
            #[cfg(unix)]
            TransportStream::Unix(stream) => {
                stream.write_all(msg.as_bytes()).await?;
                stream.flush().await?;
            }
            #[cfg(windows)]
            TransportStream::Pipe(client) => {
                client.write_all(msg.as_bytes()).await?;
                client.flush().await?;
            }
            TransportStream::Tcp(stream) => {
                stream.write_all(msg.as_bytes()).await?;
                stream.flush().await?;
            }
        }
        Ok(())
    }

    pub async fn read_response(&mut self) -> Result<Response> {
        let line = match self {
            #[cfg(unix)]
            TransportStream::Unix(stream) => {
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await?;
                line
            }
            #[cfg(windows)]
            TransportStream::Pipe(client) => {
                let mut reader = BufReader::new(client);
                let mut line = String::new();
                reader.read_line(&mut line).await?;
                line
            }
            TransportStream::Tcp(stream) => {
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await?;
                line
            }
        };

        let line = line.trim();
        if line.is_empty() {
            return Err(CoreError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Empty response",
            )));
        }

        deserialize_message(line)
            .map_err(|e| CoreError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }
}

pub struct IpcClient {
    stream: TransportStream,
}

impl IpcClient {
    pub async fn connect(config: &Config, workspace_fingerprint: &str) -> Result<Self> {
        let stream = TransportStream::connect(config, workspace_fingerprint).await?;
        Ok(Self { stream })
    }

    pub async fn send_request(&mut self, request: Request) -> Result<Response> {
        self.stream.send_request(&request).await?;
        self.stream.read_response().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type() {
        let config = Config::default();
        let transport = config.get_effective_transport();
        
        #[cfg(unix)]
        assert_eq!(transport, TransportType::Unix);
        #[cfg(windows)]
        assert_eq!(transport, TransportType::Pipe);
    }
}