#![deny(clippy::all)]
#![forbid(unsafe_code)]

use async_trait::async_trait;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

#[async_trait]
pub trait Transport: Send {
    async fn send(&mut self, message: &str) -> io::Result<()>;
    async fn receive(&mut self) -> io::Result<String>;
}

pub struct StdioTransport {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl StdioTransport {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin,
            stdout: BufReader::new(stdout),
        }
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn send(&mut self, message: &str) -> io::Result<()> {
        self.stdin.write_all(message.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await
    }

    async fn receive(&mut self) -> io::Result<String> {
        let mut line = String::new();
        self.stdout.read_line(&mut line).await?;
        Ok(line.trim().to_string())
    }
}

pub struct MemoryTransport {
    outgoing: async_channel::Sender<String>,
    incoming: async_channel::Receiver<String>,
}

impl MemoryTransport {
    pub fn new_pair() -> (Self, Self) {
        let (tx1, rx1) = async_channel::unbounded();
        let (tx2, rx2) = async_channel::unbounded();

        let transport1 = Self {
            outgoing: tx1,
            incoming: rx2,
        };

        let transport2 = Self {
            outgoing: tx2,
            incoming: rx1,
        };

        (transport1, transport2)
    }
}

#[async_trait]
impl Transport for MemoryTransport {
    async fn send(&mut self, message: &str) -> io::Result<()> {
        self.outgoing
            .send(message.to_string())
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }

    async fn receive(&mut self) -> io::Result<String> {
        self.incoming
            .recv()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }
}
