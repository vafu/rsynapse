//! Native protocol probe: `input_idle [threshold-seconds]`.
use futures_util::StreamExt;
use shell_source::rx::Observable as _;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let seconds = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "30".to_owned())
        .parse()
        .expect("seconds");
    let mut states =
        shell_source::wayland::input_idle(std::time::Duration::from_secs(seconds)).into_stream();
    while let Some(item) = states.next().await {
        match item {
            Ok(idle) => println!("{}", if idle { "idle" } else { "active" }),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
}
