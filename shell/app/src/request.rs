use std::{
    ffi::OsString,
    fmt, fs,
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

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
    SchemeToggle,
    Hints(HintsAction),
    Notifications(NotificationCenterAction),
}

impl ShellRequest {
    pub const fn target(&self) -> RequestTarget {
        match self {
            Self::SchemeToggle | Self::Hints(_) => RequestTarget::Shell,
            Self::Notifications(_) => RequestTarget::Notifications,
        }
    }
}

/// Super-key hints request action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HintsAction {
    Set(bool),
    Toggle,
}

/// Notification center request action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationCenterAction {
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
    _thread: JoinHandle<()>,
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
        let _ = fs::remove_file(&self.path);
    }
}

/// Start a local request server for the selected process.
pub fn start_server(
    target: RequestTarget,
    dispatch: impl Fn(PendingRequest) + Send + 'static,
) -> Result<RequestServer, String> {
    let path = socket_path(target);
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
    let server_path = path.clone();
    let thread = thread::Builder::new()
        .name("rsynapse-request-server".to_owned())
        .spawn(move || accept_requests(listener, dispatch))
        .map_err(|error| format!("spawn request server: {error}"))?;

    Ok(RequestServer {
        path: server_path,
        _thread: thread,
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
pub fn send_notification_center_action(
    action: NotificationCenterAction,
) -> Result<RequestResponse, String> {
    send_request(
        RequestTarget::Notifications,
        &notification_center_request_args(action),
    )
}

fn accept_requests(listener: UnixListener, dispatch: impl Fn(PendingRequest)) {
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => handle_connection(&mut stream, &dispatch),
            Err(error) => eprintln!("[request] accept failed: {error}"),
        }
    }
}

fn handle_connection(stream: &mut UnixStream, dispatch: &impl Fn(PendingRequest)) {
    let response = match read_request(stream).and_then(|args| parse_request(&args)) {
        Ok(request) => {
            let (sender, receiver) = mpsc::channel();
            dispatch(PendingRequest::new(request, sender));
            receiver
                .recv_timeout(Duration::from_secs(5))
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

fn notification_center_request_args(action: NotificationCenterAction) -> Vec<String> {
    let action = match action {
        NotificationCenterAction::Set(true) => "show",
        NotificationCenterAction::Set(false) => "hide",
        NotificationCenterAction::Toggle => "toggle",
    };

    vec!["notifications".to_owned(), action.to_owned()]
}

fn read_request(stream: &mut UnixStream) -> Result<Vec<String>, String> {
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read request: {error}"))?;
    decode_args(&bytes)
}

fn parse_request(args: &[String]) -> Result<ShellRequest, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err("missing request command".to_owned());
    };
    match command {
        "scheme-toggle" => {
            if args.len() == 1 {
                Ok(ShellRequest::SchemeToggle)
            } else {
                Err("scheme-toggle does not accept arguments".to_owned())
            }
        }
        "hints" => parse_hints_request(&args[1..]).map(ShellRequest::Hints),
        "notifications" | "notification-center" => {
            parse_notification_center_request(&args[1..]).map(ShellRequest::Notifications)
        }
        _ => Err(format!("unknown request command: {command}")),
    }
}

fn parse_notification_center_request(args: &[String]) -> Result<NotificationCenterAction, String> {
    match args {
        [action] if action == "toggle" => Ok(NotificationCenterAction::Toggle),
        [action] if action == "show" => Ok(NotificationCenterAction::Set(true)),
        [action] if action == "hide" => Ok(NotificationCenterAction::Set(false)),
        [key, value] if key == "open" || key == "active" => {
            parse_bool(value).map(NotificationCenterAction::Set)
        }
        [] => Err("notifications requires open <bool>, show, hide, or toggle".to_owned()),
        _ => Err("invalid notifications request".to_owned()),
    }
}

fn parse_hints_request(args: &[String]) -> Result<HintsAction, String> {
    match args {
        [action] if action == "toggle" => Ok(HintsAction::Toggle),
        [action] if action == "show" => Ok(HintsAction::Set(true)),
        [action] if action == "hide" => Ok(HintsAction::Set(false)),
        [key, value] if key == "active" => parse_bool(value).map(HintsAction::Set),
        [] => Err("hints requires active <bool>, show, hide, or toggle".to_owned()),
        _ => Err("invalid hints request".to_owned()),
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
        HintsAction, NotificationCenterAction, RequestResponse, RequestTarget, ShellRequest,
        decode_args, encode_args, notification_center_request_args, parse_request, parse_response,
        socket_path,
    };

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_scheme_toggle() {
        assert_eq!(
            parse_request(&args(&["scheme-toggle"])).unwrap(),
            ShellRequest::SchemeToggle
        );
    }

    #[test]
    fn parses_hints_active_bool() {
        assert_eq!(
            parse_request(&args(&["hints", "active", "true"])).unwrap(),
            ShellRequest::Hints(HintsAction::Set(true))
        );
        assert_eq!(
            parse_request(&args(&["hints", "active", "false"])).unwrap(),
            ShellRequest::Hints(HintsAction::Set(false))
        );
    }

    #[test]
    fn parses_hints_toggle() {
        assert_eq!(
            parse_request(&args(&["hints", "toggle"])).unwrap(),
            ShellRequest::Hints(HintsAction::Toggle)
        );
    }

    #[test]
    fn parses_notifications_actions() {
        assert_eq!(
            parse_request(&args(&["notifications", "toggle"])).unwrap(),
            ShellRequest::Notifications(NotificationCenterAction::Toggle)
        );
        assert_eq!(
            parse_request(&args(&["notifications", "show"])).unwrap(),
            ShellRequest::Notifications(NotificationCenterAction::Set(true))
        );
        assert_eq!(
            parse_request(&args(&["notification-center", "hide"])).unwrap(),
            ShellRequest::Notifications(NotificationCenterAction::Set(false))
        );
    }

    #[test]
    fn routes_requests_to_process_targets() {
        assert_eq!(
            parse_request(&args(&["scheme-toggle"])).unwrap().target(),
            RequestTarget::Shell
        );
        assert_eq!(
            parse_request(&args(&["hints", "toggle"])).unwrap().target(),
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
            notification_center_request_args(NotificationCenterAction::Toggle),
            args(&["notifications", "toggle"])
        );
        assert_eq!(
            notification_center_request_args(NotificationCenterAction::Set(true)),
            args(&["notifications", "show"])
        );
        assert_eq!(
            notification_center_request_args(NotificationCenterAction::Set(false)),
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
        let input = args(&["hints", "active", "true"]);
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
