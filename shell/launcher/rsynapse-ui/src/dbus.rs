use rsynapse_launcher_api::{
    EngineProxyBlocking, KeyRequest, KeyResponse, SearchRequest, SearchResponse,
};
use zbus::blocking::Connection;

fn connection() -> anyhow::Result<Connection> {
    Ok(Connection::session()?)
}

pub fn search(request: SearchRequest) -> anyhow::Result<SearchResponse> {
    let connection = connection()?;
    let proxy = EngineProxyBlocking::new(&connection)?;
    Ok(proxy.search(request)?)
}

pub fn handle_key(request: KeyRequest) -> anyhow::Result<KeyResponse> {
    let connection = connection()?;
    let proxy = EngineProxyBlocking::new(&connection)?;
    Ok(proxy.handle_key(request)?)
}

pub fn execute(id: &str) -> anyhow::Result<String> {
    let connection = connection()?;
    let proxy = EngineProxyBlocking::new(&connection)?;
    Ok(proxy.execute(id)?)
}
