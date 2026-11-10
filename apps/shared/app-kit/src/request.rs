use std::{
    ffi::OsString,
    fmt, fs,
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const REQUEST_WORKERS: usize = 4;
const REQUEST_QUEUE: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Runtime process that can receive rsynapse shell requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestTarget {
    Shell,
    Notifications,
}

impl RequestTarget {
    const fn socket_dir(self) -> &'static str {
        match self {
            Self::Shell => "rsynapse-shell",
            Self::Notifications => "rsynapse-notifications",
        }
    }
}

/// Product-level request understood by one of the rsynapse shell processes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellRequest {
    ProjectInit,
    WorkspaceSelector(ToggleAction),
    Mpris { action: ToggleAction, hold: bool },
    Notifications(ToggleAction),
}

impl ShellRequest {
    pub const fn target(&self) -> RequestTarget {
        match self {
            Self::ProjectInit | Self::WorkspaceSelector(_) | Self::Mpris { .. } => {
                RequestTarget::Shell
            }
            Self::Notifications(_) => RequestTarget::Notifications,
        }
    }
}

/// Show/hide/toggle action shared by overlay surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToggleAction {
    Set(bool),
    Toggle,
}

/// Response returned over the local request socket.
#[derive(Debug)]
pub enum RequestResponse {
    Ok,
    Error(String),
}

/// Request plus a one-shot response channel owned by a request server.
pub struct PendingRequest {
    pub(crate) request: ShellRequest,
    response: mpsc::Sender<RequestResponse>,
}

impl PendingRequest {
    fn new(request: ShellRequest, response: mpsc::Sender<RequestResponse>) -> Self {
        Self { request, response }
    }

    pub fn request(&self) -> &ShellRequest {
        &self.request
    }

    pub fn respond(self, response: RequestResponse) {
        let _ = self.response.send(response);
    }
}

impl fmt::Debug for PendingRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingRequest")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

/// Long-lived local request socket server.
pub struct RequestServer {
    path: PathBuf,
    shutdown: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl fmt::Debug for RequestServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestServer")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Drop for RequestServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        // Wake the blocking accept loop so it can observe shutdown.
        let _ = UnixStream::connect(&self.path);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.path);
    }
}

/// Start a local request server for the selected process.
pub fn start_server(
    target: RequestTarget,
    dispatch: impl Fn(PendingRequest) + Send + Sync + 'static,
) -> Result<RequestServer, String> {
    start_server_at(socket_path(target), dispatch)
}

fn start_server_at(
    path: PathBuf,
    dispatch: impl Fn(PendingRequest) + Send + Sync + 'static,
) -> Result<RequestServer, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("request socket path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "create request socket directory {}: {error}",
            parent.display()
        )
    })?;
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            return Err(format!(
                "request socket is already active at {}",
                path.display()
            ));
        }
        fs::remove_file(&path)
            .map_err(|error| format!("remove stale request socket {}: {error}", path.display()))?;
    }
    let listener = UnixListener::bind(&path)
        .map_err(|error| format!("bind request socket {}: {error}", path.display()))?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let dispatch = Arc::new(dispatch);
    let (sender, receiver) = mpsc::sync_channel::<UnixStream>(REQUEST_QUEUE);
    let receiver = Arc::new(Mutex::new(receiver));

    let mut threads = Vec::with_capacity(REQUEST_WORKERS + 1);
    for index in 0..REQUEST_WORKERS {
        let receiver = receiver.clone();
        let dispatch = dispatch.clone();
        let worker = thread::Builder::new()
            .name(format!("rsynapse-request-worker-{index}"))
            .spawn(move || worker_loop(&receiver, dispatch.as_ref()))
            .map_err(|error| format!("spawn request worker: {error}"))?;
        threads.push(worker);
    }

    let accept_shutdown = shutdown.clone();
    let server_path = path.clone();
    let accept = thread::Builder::new()
        .name("rsynapse-request-server".to_owned())
        .spawn(move || accept_requests(listener, sender, &accept_shutdown))
        .map_err(|error| format!("spawn request server: {error}"))?;
    threads.push(accept);

    Ok(RequestServer {
        path: server_path,
        shutdown,
        threads,
    })
}

/// Handle `rsynapse-shell request ...` style command-line invocations.
pub fn run_cli(args: impl IntoIterator<Item = OsString>) -> i32 {
    let args = match os_args_to_strings(args) {
        Ok(args) if !args.is_empty() => args,
        Ok(_) => {
            eprintln!("usage: rsynapse-shell request <command> [key value ...]");
            return 2;
        }
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };

    let request = match parse_request(&args) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };

    match send_request(request.target(), &args) {
        Ok(RequestResponse::Ok) => {
            println!("ok");
            0
        }
        Ok(RequestResponse::Error(error)) => {
            eprintln!("{error}");
            1
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

/// Send a notification center action to the notifications process.
pub fn send_notification_center_action(action: ToggleAction) -> Result<RequestResponse, String> {
    send_request(
        RequestTarget::Notifications,
        &notification_center_request_args(action),
    )
}

fn accept_requests(
    listener: UnixListener,
    sender: mpsc::SyncSender<UnixStream>,
    shutdown: &AtomicBool,
) {
    for stream in listener.incoming() {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match stream {
            Ok(stream) => {
                if sender.send(stream).is_err() {
                    break;
                }
            }
            Err(error) => eprintln!("[request] accept failed: {error}"),
        }
    }
}

fn worker_loop(receiver: &Mutex<mpsc::Receiver<UnixStream>>, dispatch: &impl Fn(PendingRequest)) {
    loop {
        let stream = receiver
            .lock()
            .expect("request worker receiver poisoned")
            .recv();
        match stream {
            Ok(mut stream) => handle_connection(&mut stream, dispatch),
            Err(_) => break,
        }
    }
}

fn handle_connection(stream: &mut UnixStream, dispatch: &impl Fn(PendingRequest)) {
    let response = match read_request(stream).and_then(|args| parse_request(&args)) {
        Ok(request) => {
            let (sender, receiver) = mpsc::channel();
            dispatch(PendingRequest::new(request, sender));
            receiver
                .recv_timeout(REQUEST_TIMEOUT)
                .unwrap_or_else(|_| RequestResponse::Error("request timed out".to_owned()))
        }
        Err(error) => RequestResponse::Error(error),
    };
    let _ = stream.write_all(response_line(&response).as_bytes());
}

fn send_request(target: RequestTarget, args: &[String]) -> Result<RequestResponse, String> {
    let path = socket_path(target);
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| format!("connect request socket {}: {error}", path.display()))?;
    stream
        .write_all(&encode_args(args))
        .map_err(|error| format!("write request: {error}"))?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| format!("finish request write: {error}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read response: {error}"))?;
    parse_response(&response)
}

fn notification_center_request_args(action: ToggleAction) -> Vec<String> {
    let action = match action {
        ToggleAction::Set(true) => "show",
        ToggleAction::Set(false) => "hide",
        ToggleAction::Toggle => "toggle",
    };

    vec!["notifications".to_owned(), action.to_owned()]
}

fn read_request(stream: &mut UnixStream) -> Result<Vec<String>, String> {
    let mut bytes = Vec::new();
    let mut limited = stream.take(MAX_REQUEST_BYTES + 1);
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read request: {error}"))?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        // Drain a bounded remainder so closing the socket does not reset the
        // connection before the client reads the error response.
        let mut rest = limited.into_inner().take(MAX_REQUEST_BYTES);
        let _ = std::io::copy(&mut rest, &mut std::io::sink());
        return Err(format!("request exceeds {MAX_REQUEST_BYTES} byte limit"));
    }
    decode_args(&bytes)
}

fn parse_request(args: &[String]) -> Result<ShellRequest, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err("missing request command".to_owned());
    };
    match command {
        "project-init" if args.len() == 1 => Ok(ShellRequest::ProjectInit),
        "workspace-selector" | "workspace-picker" => {
            parse_toggle_request(&args[1..], "workspace-selector", true)
                .map(ShellRequest::WorkspaceSelector)
        }
        "mpris" | "media" => parse_mpris_request(&args[1..]),
        "notifications" | "notification-center" => {
            parse_toggle_request(&args[1..], "notifications", false)
                .map(ShellRequest::Notifications)
        }
        _ => Err(format!("unknown request command: {command}")),
    }
}

fn parse_mpris_request(args: &[String]) -> Result<ShellRequest, String> {
    let (args, hold) = match args {
        [rest @ .., flag] if flag == "--hold" => (rest, true),
        [rest @ .., flag] if flag == "--no-hold" => (rest, false),
        _ => (args, true),
    };
    let action = parse_toggle_request(args, "mpris", true)?;
    Ok(ShellRequest::Mpris { action, hold })
}

fn parse_toggle_request(
    args: &[String],
    kind: &str,
    empty_toggles: bool,
) -> Result<ToggleAction, String> {
    match args {
        [] if empty_toggles => Ok(ToggleAction::Toggle),
        [] => Err(format!(
            "{kind} requires open <bool>, show, hide, or toggle"
        )),
        [action] if action == "toggle" => Ok(ToggleAction::Toggle),
        [action] if action == "show" => Ok(ToggleAction::Set(true)),
        [action] if action == "hide" => Ok(ToggleAction::Set(false)),
        [key, value] if key == "open" || key == "active" => {
            parse_bool(value).map(ToggleAction::Set)
        }
        _ => Err(format!(
            "{kind} requires open <bool>, show, hide, or toggle"
        )),
    }
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("expected boolean, got {value:?}")),
    }
}

fn parse_response(response: &str) -> Result<RequestResponse, String> {
    let response = response.trim_end_matches('\n');
    if response == "ok" {
        return Ok(RequestResponse::Ok);
    }
    if let Some(error) = response.strip_prefix("error ") {
        return Ok(RequestResponse::Error(error.to_owned()));
    }
    Err(format!("invalid response: {response:?}"))
}

fn response_line(response: &RequestResponse) -> String {
    match response {
        RequestResponse::Ok => "ok\n".to_owned(),
        RequestResponse::Error(error) => format!("error {error}\n"),
    }
}

fn encode_args(args: &[String]) -> Vec<u8> {
    args.join("\0").into_bytes()
}

fn decode_args(bytes: &[u8]) -> Result<Vec<String>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    bytes
        .split(|byte| *byte == 0)
        .map(|arg| {
            std::str::from_utf8(arg)
                .map(str::to_owned)
                .map_err(|error| format!("request argument is not UTF-8: {error}"))
        })
        .collect()
}

fn os_args_to_strings(args: impl IntoIterator<Item = OsString>) -> Result<Vec<String>, String> {
    args.into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|arg| format!("request argument is not UTF-8: {arg:?}"))
        })
        .collect()
}

fn socket_path(target: RequestTarget) -> PathBuf {
    runtime_dir().join(target.socket_dir()).join("request.sock")
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_REQUEST_BYTES, RequestResponse, RequestTarget, ShellRequest, ToggleAction, decode_args,
        encode_args, notification_center_request_args, parse_request, parse_response, socket_path,
        start_server_at,
    };
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, AtomicUsize, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_socket(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rsynapse-request-{}-{name}-{}.sock",
            std::process::id(),
            SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn send_raw(path: &PathBuf, bytes: &[u8]) -> String {
        let mut stream = UnixStream::connect(path).unwrap();
        stream.write_all(bytes).unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn oversized_requests_are_rejected() {
        let path = test_socket("oversized");
        let server =
            start_server_at(path.clone(), |request| request.respond(RequestResponse::Ok)).unwrap();
        let response = send_raw(&path, &vec![b'a'; (MAX_REQUEST_BYTES + 8) as usize]);

        assert!(response.contains("exceeds"), "{response}");
        drop(server);
    }

    #[test]
    fn dropping_the_server_stops_the_accept_loop_and_removes_the_socket() {
        let path = test_socket("drop");
        let server =
            start_server_at(path.clone(), |request| request.respond(RequestResponse::Ok)).unwrap();
        assert!(path.exists());

        drop(server);

        assert!(!path.exists());
    }

    #[test]
    fn a_slow_handler_does_not_block_another_connection() {
        let path = test_socket("slow");
        let calls = Arc::new(AtomicUsize::new(0));
        let dispatch_calls = calls.clone();
        let server = start_server_at(path.clone(), move |request| {
            if dispatch_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                thread::sleep(Duration::from_millis(1_200));
            }
            request.respond(RequestResponse::Ok)
        })
        .unwrap();

        let slow_path = path.clone();
        let slow = thread::spawn(move || send_raw(&slow_path, b"workspace-selector\0toggle"));
        thread::sleep(Duration::from_millis(100));

        let started = Instant::now();
        let fast = send_raw(&path, b"workspace-selector\0toggle");
        assert!(started.elapsed() < Duration::from_millis(700));
        assert_eq!(fast.trim_end(), "ok");

        slow.join().unwrap();
        drop(server);
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_notifications_actions() {
        assert_eq!(
            parse_request(&args(&["notifications", "toggle"])).unwrap(),
            ShellRequest::Notifications(ToggleAction::Toggle)
        );
        assert_eq!(
            parse_request(&args(&["notifications", "show"])).unwrap(),
            ShellRequest::Notifications(ToggleAction::Set(true))
        );
        assert_eq!(
            parse_request(&args(&["notification-center", "hide"])).unwrap(),
            ShellRequest::Notifications(ToggleAction::Set(false))
        );
    }

    #[test]
    fn routes_workspace_requests_to_the_shell() {
        assert_eq!(
            parse_request(&args(&["workspace-selector", "show"])).unwrap(),
            ShellRequest::WorkspaceSelector(ToggleAction::Set(true))
        );
        assert_eq!(
            parse_request(&args(&["workspace-selector"])).unwrap(),
            ShellRequest::WorkspaceSelector(ToggleAction::Toggle)
        );
        assert_eq!(
            parse_request(&args(&["workspace-picker", "hide"])).unwrap(),
            ShellRequest::WorkspaceSelector(ToggleAction::Set(false))
        );
        assert_eq!(
            parse_request(&args(&["workspace-selector", "toggle"]))
                .unwrap()
                .target(),
            RequestTarget::Shell
        );
        assert!(parse_request(&args(&["workspace-selector", "invalid"])).is_err());
    }

    #[test]
    fn parses_mpris_hold_requests() {
        assert_eq!(
            parse_request(&args(&["mpris", "show", "--hold"])).unwrap(),
            ShellRequest::Mpris {
                action: ToggleAction::Set(true),
                hold: true,
            }
        );
        assert_eq!(
            parse_request(&args(&["media", "toggle", "--no-hold"])).unwrap(),
            ShellRequest::Mpris {
                action: ToggleAction::Toggle,
                hold: false,
            }
        );
        assert_eq!(
            parse_request(&args(&["mpris"])).unwrap(),
            ShellRequest::Mpris {
                action: ToggleAction::Toggle,
                hold: true,
            }
        );
        assert!(parse_request(&args(&["mpris", "invalid"])).is_err());
    }

    #[test]
    fn routes_requests_to_process_targets() {
        assert_eq!(
            parse_request(&args(&["project-init"])).unwrap(),
            ShellRequest::ProjectInit
        );
        assert_eq!(ShellRequest::ProjectInit.target(), RequestTarget::Shell);
        assert!(parse_request(&args(&["project-init", "extra"])).is_err());
        assert_eq!(
            parse_request(&args(&["workspace-selector", "toggle"]))
                .unwrap()
                .target(),
            RequestTarget::Shell
        );
        assert_eq!(
            parse_request(&args(&["notifications", "toggle"]))
                .unwrap()
                .target(),
            RequestTarget::Notifications
        );
    }

    #[test]
    fn builds_notification_center_request_args() {
        assert_eq!(
            notification_center_request_args(ToggleAction::Toggle),
            args(&["notifications", "toggle"])
        );
        assert_eq!(
            notification_center_request_args(ToggleAction::Set(true)),
            args(&["notifications", "show"])
        );
        assert_eq!(
            notification_center_request_args(ToggleAction::Set(false)),
            args(&["notifications", "hide"])
        );
    }

    #[test]
    fn separates_socket_paths_by_target() {
        assert!(socket_path(RequestTarget::Shell).ends_with("rsynapse-shell/request.sock"),);
        assert!(
            socket_path(RequestTarget::Notifications)
                .ends_with("rsynapse-notifications/request.sock"),
        );
    }

    #[test]
    fn rejects_unknown_commands() {
        assert!(parse_request(&args(&["unknown"])).is_err());
    }

    #[test]
    fn round_trips_request_args() {
        let input = args(&["notifications", "show"]);
        assert_eq!(decode_args(&encode_args(&input)).unwrap(), input);
    }

    #[test]
    fn parses_response_lines() {
        assert!(matches!(
            parse_response("ok\n").unwrap(),
            RequestResponse::Ok
        ));
        assert!(matches!(
            parse_response("error nope\n").unwrap(),
            RequestResponse::Error(error) if error == "nope"
        ));
    }
}
