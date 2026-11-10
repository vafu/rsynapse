use niri_ipc::{Event, Reply, Request, Response};
use std::path::Path;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

pub struct Socket(BufReader<UnixStream>);
impl Socket {
    pub async fn connect(path: &Path) -> std::io::Result<Self> {
        Ok(Self(BufReader::new(UnixStream::connect(path).await?)))
    }
    pub async fn send(&mut self, request: Request) -> anyhow::Result<Response> {
        let mut data = serde_json::to_vec(&request)?;
        data.push(b'\n');
        self.0.get_mut().write_all(&data).await?;
        let reply: Reply = serde_json::from_str(&self.line().await?)?;
        reply.map_err(|error| anyhow::anyhow!("Niri rejected request: {error}"))
    }
    pub async fn finish_requests(&mut self) -> std::io::Result<()> {
        self.0.get_mut().shutdown().await
    }
    pub async fn event(&mut self) -> anyhow::Result<Event> {
        Ok(serde_json::from_str(&self.line().await?)?)
    }
    async fn line(&mut self) -> anyhow::Result<String> {
        let mut data = String::new();
        anyhow::ensure!(self.0.read_line(&mut data).await? > 0, "Niri socket closed");
        Ok(data)
    }
}
