//! Read the first live Listener membership (ALL supplied labels; none = all PRs).
//! cargo run -p shell-source --example github_listener -- label-name
use futures_util::StreamExt;
use shell_source::{github, rx::Observable as _};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let labels = std::env::args().skip(1).collect();
    let mut membership = github::listener(labels).into_stream();
    let urls = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(value) = membership.next().await {
            if let Some(urls) = value? {
                return Ok::<_, String>(urls);
            }
        }
        Err("Listener source ended without a membership snapshot".to_owned())
    })
    .await??;
    println!("{}", serde_json::to_string(&urls)?);
    // Unsubscribe while the runtime is alive; creator disconnection also reaps
    // the object if the process exits before its no-reply Close is dispatched.
    drop(membership);
    Ok(())
}
