use std::path::Path;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

pub(crate) async fn command(path: Option<&Path>, action: &str) -> anyhow::Result<()> {
    let path = super::socket::socket_path(
        path,
        std::env::var_os("UMBRIEL_SOCKET").as_deref(),
        std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
    )?;
    let mut stream = UnixStream::connect(path).await?;
    let mut bytes = serde_json::to_vec(&serde_json::json!({"cmd":"msg", "arg":action}))?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).await?;
    let value: serde_json::Value = serde_json::from_str(&reply)?;
    anyhow::ensure!(
        value.get("ok").is_some(),
        "Umbriel rejected {action}: {value}"
    );
    Ok(())
}
