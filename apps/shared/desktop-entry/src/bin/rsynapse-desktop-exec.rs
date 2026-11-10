//! Prints the executable for a desktop ID using the shared desktop-entry parser.
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(id) = std::env::args().nth(1) else {
        eprintln!("usage: rsynapse-desktop-exec DESKTOP_ID");
        return ExitCode::from(64);
    };
    let id = id.strip_suffix(".desktop").unwrap_or(&id);
    let Some(entry) = desktop_entry::find(id) else {
        return ExitCode::FAILURE;
    };
    let Some(argv) = entry.exec_argv(id) else {
        return ExitCode::FAILURE;
    };
    let mut arguments = argv.iter();
    let Some(first) = arguments.next() else {
        return ExitCode::FAILURE;
    };
    let program = if first == "env" {
        arguments
            .by_ref()
            .find(|argument| !argument.contains('='))
            .map(String::as_str)
            .unwrap_or_default()
    } else {
        first.as_str()
    };
    if program.is_empty() {
        return ExitCode::FAILURE;
    }
    println!("{program}");
    ExitCode::SUCCESS
}
