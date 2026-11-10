use std::sync::OnceLock;

use rsynapse_launcher_api::{
    EngineProxyBlocking, KeyRequest, KeyResponse, SearchRequest, SearchResponse,
};
use zbus::blocking::Connection;

static CONNECTION: OnceLock<Connection> = OnceLock::new();

fn connection() -> anyhow::Result<&'static Connection> {
    if let Some(connection) = CONNECTION.get() {
        return Ok(connection);
    }
    let connection = Connection::session()?;
    let _ = CONNECTION.set(connection);
    Ok(CONNECTION.get().expect("connection was just set"))
}

pub fn search(request: SearchRequest) -> anyhow::Result<SearchResponse> {
    let proxy = EngineProxyBlocking::new(connection()?)?;
    Ok(proxy.search(request)?)
}

pub fn handle_key(request: KeyRequest) -> anyhow::Result<KeyResponse> {
    let proxy = EngineProxyBlocking::new(connection()?)?;
    Ok(proxy.handle_key(request)?)
}

pub fn execute(id: &str) -> anyhow::Result<String> {
    let proxy = EngineProxyBlocking::new(connection()?)?;
    Ok(proxy.execute(id)?)
}
