use zbus::blocking::{Connection, Proxy};

const SERVICE: &str = "com.rsynapse.Engine";
const PATH: &str = "/org/rsynapse/Engine1";
const INTERFACE: &str = "org.rsynapse.Engine1";

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub data: String,
}

fn proxy() -> anyhow::Result<Proxy<'static>> {
    let conn = Connection::session()?;
    let proxy = Proxy::new(&conn, SERVICE, PATH, INTERFACE)?;
    Ok(proxy)
}

pub fn search(query: &str) -> anyhow::Result<Vec<SearchResult>> {
    let proxy = proxy()?;
    let reply: Vec<(String, String, String, String, String)> =
        proxy.call("Search", &(query,))?;
    Ok(reply
        .into_iter()
        .map(|(id, title, description, icon, data)| SearchResult {
            id,
            title,
            description,
            icon,
            data,
        })
        .collect())
}

pub fn execute(id: &str) -> anyhow::Result<String> {
    let proxy = proxy()?;
    Ok(proxy.call("Execute", &(id,))?)
}
