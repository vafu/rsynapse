use std::{env, io};

use niri_ipc::{Event, Reply, Request, Response, socket::SOCKET_PATH_ENV};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

pub struct AsyncNiriSocket {
    stream: BufReader<UnixStream>,
}

impl AsyncNiriSocket {
    pub async fn connect() -> io::Result<Self> {
        let socket_path = env::var_os(SOCKET_PATH_ENV).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{SOCKET_PATH_ENV} is not set, are you running this within niri?"),
            )
        })?;
        let stream = UnixStream::connect(socket_path).await?;
        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    pub async fn send(&mut self, request: Request) -> anyhow::Result<Response> {
        match self.send_raw(request).await? {
            Ok(response) => Ok(response),
            Err(message) => anyhow::bail!("niri rejected IPC request: {message}"),
        }
    }

    async fn send_raw(&mut self, request: Request) -> io::Result<Reply> {
        let mut request = serde_json::to_string(&request)?;
        request.push('\n');
        self.stream.get_mut().write_all(request.as_bytes()).await?;
        self.stream.get_mut().flush().await?;

        let mut response = String::new();
        let bytes = self.stream.read_line(&mut response).await?;
        if bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "niri IPC response stream closed",
            ));
        }
        serde_json::from_str(&response).map_err(Into::into)
    }

    pub async fn start_event_stream(mut self) -> anyhow::Result<Self> {
        match self.send(Request::EventStream).await? {
            Response::Handled => {}
            response => anyhow::bail!("unexpected niri event stream response: {response:?}"),
        }
        self.stream.get_mut().shutdown().await?;
        Ok(self)
    }

    pub async fn read_event(&mut self) -> io::Result<Event> {
        let mut event = String::new();
        let bytes = self.stream.read_line(&mut event).await?;
        if bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "niri event stream closed",
            ));
        }
        serde_json::from_str(&event).map_err(Into::into)
    }
}

pub async fn initial_snapshot()
-> anyhow::Result<(String, std::collections::HashMap<String, niri_ipc::Output>)> {
    let mut socket = AsyncNiriSocket::connect().await?;
    let version = match socket.send(Request::Version).await? {
        Response::Version(version) => version,
        response => anyhow::bail!("unexpected niri version response: {response:?}"),
    };
    let outputs = match socket.send(Request::Outputs).await? {
        Response::Outputs(outputs) => outputs,
        response => anyhow::bail!("unexpected niri outputs response: {response:?}"),
    };
    Ok((version, outputs))
}

pub async fn event_stream() -> anyhow::Result<AsyncNiriSocket> {
    AsyncNiriSocket::connect().await?.start_event_stream().await
}
