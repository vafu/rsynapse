//! Prints live emissions from the github-dbus sources for a few seconds.
//! Run: cargo run -p shell-source --example github_prs

use shell_source::{
    github,
    rx::{Observable as _, Observer},
};

struct Print<F>(F);

impl<T, F: FnMut(T)> Observer<T, String> for Print<F> {
    fn next(&mut self, value: T) {
        (self.0)(value);
    }

    fn error(self, error: String) {
        eprintln!("source error: {error}");
    }

    fn complete(self) {}

    fn is_closed(&self) -> bool {
        false
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let _overview = github::overview().subscribe_with(Print(|overview| {
        println!("overview: {overview:?}");
    }));
    let _summary = github::summary().subscribe_with(Print(|summary| {
        println!("summary: {summary:?}");
    }));
    let _prs = github::pull_requests().subscribe_with(Print(|prs: Option<Vec<github_dbus_api::PullRequest>>| {
        let prs = prs.unwrap_or_default();
        let merging: Vec<_> = prs
            .iter()
            .filter(|pr| pr.status == github_dbus_api::PrStatus::Merging)
            .map(|pr| pr.number)
            .collect();
        println!("prs: {} total, merging: {merging:?}", prs.len());
    }));
    let _notifications = github::notifications().subscribe_with(Print(|event: github_dbus_api::NotificationEvent| {
        println!("notification: [{}] {}", event.repo, event.title);
    }));

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
}
