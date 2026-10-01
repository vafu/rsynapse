//! # Graphyne
//!
//! A simple, reliable Rust client for sending metrics to [Graphite](https://graphiteapp.org/) Carbon daemons.
//!
//! Graphyne provides a straightforward API for sending time-series metrics to Graphite using either
//! the plaintext TCP or UDP protocol. It features automatic reconnection (TCP), configurable retry
//! logic, and an ergonomic builder pattern for easy configuration.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use graphyne::{GraphiteClient, GraphiteMessage, Protocol};
//! use std::time::Duration;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a TCP client (default)
//! let mut client = GraphiteClient::builder()
//!     .address("127.0.0.1")
//!     .port(2003)
//!     .build()?;
//!
//! // Or create a UDP client
//! let mut udp_client = GraphiteClient::builder()
//!     .address("127.0.0.1")
//!     .port(2003)
//!     .protocol(Protocol::Udp)
//!     .build()?;
//!
//! // Send a metric
//! let message = GraphiteMessage::new("my.metric.path", "42");
//! client.send_message(&message)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Features
//!
//! - **Builder Pattern**: Intuitive, type-safe configuration
//! - **TCP & UDP Support**: Choose between reliable TCP or high-performance UDP
//! - **Auto-reconnection**: Automatic retry and reconnection on failure (TCP)
//! - **Zero-copy Writes**: Efficient metric transmission
//! - **Timestamp Generation**: Automatic Unix timestamp creation
//!
//! ## Protocol
//!
//! Graphyne uses the Graphite plaintext protocol over TCP or UDP. Each metric is formatted as:
//! ```text
//! metric.path.name value timestamp\n
//! ```
//!
//! For example:
//! ```text
//! servers.web01.cpu.usage 45.2 1609459200\n
//! ```

use bon::bon;
use std::{
    fmt,
    io::{Error, Write},
    net::{AddrParseError, IpAddr, SocketAddr, TcpStream, UdpSocket},
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Default number of retry attempts for connection and send operations.
///
/// If a connection or send fails, the client will retry up to this many times
/// before returning an error.
const DEFAULT_RETRIES: u8 = 3;

/// Default timeout duration for TCP connection attempts.
///
/// This timeout applies to both initial connections and reconnection attempts.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default time to live for TCP packets
const DEFAULT_TCP_TTL: Duration = Duration::from_secs(240);

/// Protocol to use for sending metrics to Graphite.
///
/// # TCP vs UDP
///
/// - **TCP**: Reliable delivery with connection management and automatic retry.
///   Use for critical metrics where delivery must be guaranteed.
/// - **UDP**: Fire-and-forget delivery with no connection overhead.
///   Use for high-throughput scenarios where occasional metric loss is acceptable.
///
/// # Examples
///
/// ```rust,no_run
/// use graphyne::{GraphiteClient, Protocol};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // TCP for critical metrics
/// let tcp_client = GraphiteClient::builder()
///     .address("127.0.0.1")
///     .port(2003)
///     .protocol(Protocol::Tcp)
///     .build()?;
///
/// // UDP for high-volume metrics
/// let udp_client = GraphiteClient::builder()
///     .address("127.0.0.1")
///     .port(2003)
///     .protocol(Protocol::Udp)
///     .build()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
}

/// Connection type for the Graphite client.
#[derive(Debug)]
enum Connection {
    Tcp(TcpStream),
    Udp(UdpSocket),
}

/// A client for sending metrics to a Graphite Carbon daemon.
///
/// `GraphiteClient` maintains a connection (TCP) or socket (UDP) to a Graphite server and provides
/// methods for sending metrics. For TCP, it automatically handles connection failures with
/// configurable retry logic.
///
/// # Protocol Selection
///
/// The client supports both TCP and UDP protocols:
/// - **TCP** (default): Provides reliable delivery with automatic reconnection
/// - **UDP**: Provides higher throughput with no connection overhead
///
/// # Connection Management
///
/// **TCP**: The client maintains a persistent connection which is automatically reestablished if it
/// fails. When `send_message` encounters a connection error, it will attempt to reconnect
/// up to `retries` times before failing.
///
/// **UDP**: No connection management is needed; packets are sent directly without retries.
///
/// # Thread Safety
///
/// `GraphiteClient` is **not** thread-safe due to the mutable reference required by `send_message`.
/// For concurrent access, wrap it in a `Mutex` or use multiple client instances.
///
/// # Examples
///
/// ## TCP Usage
///
/// ```rust,no_run
/// use graphyne::{GraphiteClient, GraphiteMessage, Protocol};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut client = GraphiteClient::builder()
///     .address("127.0.0.1")
///     .port(2003)
///     .protocol(Protocol::Tcp)
///     .build()?;
///
/// let msg = GraphiteMessage::new("app.requests", "100");
/// client.send_message(&msg)?;
/// # Ok(())
/// # }
/// ```
///
/// ## UDP Usage
///
/// ```rust,no_run
/// use graphyne::{GraphiteClient, GraphiteMessage, Protocol};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut client = GraphiteClient::builder()
///     .address("127.0.0.1")
///     .port(2003)
///     .protocol(Protocol::Udp)
///     .build()?;
///
/// let msg = GraphiteMessage::new("app.requests", "100");
/// client.send_message(&msg)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct GraphiteClient {
    /// The connection to the Graphite server.
    connection: Connection,

    /// Socket address used for reconnection attempts and UDP sends.
    sock_addr: SocketAddr,

    /// Original address string (currently unused but reserved for future DNS support).
    _address: String,

    /// Original port number (currently unused but reserved for future use).
    _port: u16,

    /// Protocol being used (TCP or UDP).
    protocol: Protocol,

    /// Number of times to retry failed operations (TCP only).
    ///
    /// This applies to both connection attempts and send operations. A value of 3
    /// means up to 4 total attempts (1 initial + 3 retries).
    retries: u8,

    /// Timeout duration for connection attempts (TCP only).
    ///
    /// This timeout is applied to each individual connection attempt during both
    /// initial connection and reconnection operations.
    timeout: Duration,

    /// Time to live for tcp packets (TCP only).
    tcp_ttl: Duration,
}

#[bon]
impl GraphiteClient {
    /// Creates a new `GraphiteClient` using the builder pattern.
    ///
    /// This constructor establishes an initial connection (TCP) or creates a socket (UDP)
    /// to the Graphite server. If the operation fails, it returns a `GraphiteError`.
    ///
    /// # Arguments
    ///
    /// * `address` - IP address of the Graphite server (IPv4 or IPv6). **Note**: DNS hostnames
    ///   are not currently supported.
    /// * `port` - Port number where the Carbon daemon is listening (typically 2003 for TCP,
    ///   2003 for UDP plaintext)
    /// * `protocol` - Protocol to use: `Protocol::Tcp` (default) or `Protocol::Udp`
    /// * `retries` - Number of retry attempts for failed operations (default: 3, TCP only)
    /// * `timeout` - Maximum duration to wait for connection attempts (default: 5 seconds, TCP only)
    /// * `tcp_ttl` - Time to live for TCP packets (default: 240 seconds, TCP only)
    ///
    /// # Returns
    ///
    /// Returns `Ok(GraphiteClient)` if successful, or `Err(GraphiteError)` if:
    /// - The address cannot be parsed as an IP address
    /// - The connection times out (TCP)
    /// - The connection is refused (TCP)
    /// - The socket cannot be created (UDP)
    ///
    /// # Examples
    ///
    /// ## TCP with defaults
    ///
    /// ```rust,no_run
    /// use graphyne::GraphiteClient;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = GraphiteClient::builder()
    ///     .address("10.0.0.5")
    ///     .port(2003)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## UDP for high-throughput
    ///
    /// ```rust,no_run
    /// use graphyne::{GraphiteClient, Protocol};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = GraphiteClient::builder()
    ///     .address("10.0.0.5")
    ///     .port(2003)
    ///     .protocol(Protocol::Udp)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## TCP with custom retry and timeout
    ///
    /// ```rust,no_run
    /// use graphyne::{GraphiteClient, Protocol};
    /// use std::time::Duration;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = GraphiteClient::builder()
    ///     .address("192.168.1.100")
    ///     .port(2003)
    ///     .protocol(Protocol::Tcp)
    ///     .retries(10)
    ///     .timeout(Duration::from_millis(500))
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    #[builder]
    pub fn new(
        /// IP address of the Graphite server (IPv4 or IPv6).
        ///
        /// **Note**: DNS hostnames are not currently supported.
        address: impl Into<String>,
        /// Port number where the Carbon daemon is listening (typically 2003)
        port: u16,
        /// Protocol to use: TCP (default) or UDP
        #[builder(default = Protocol::default())]
        protocol: Protocol,
        /// Number of times to retry failed operations (TCP only).
        ///
        /// This applies to both connection attempts and send operations.
        /// A value of 3 means up to 4 total attempts (1 initial + 3 retries).
        #[builder(default = DEFAULT_RETRIES)]
        retries: u8,
        /// Timeout duration for connection attempts (TCP only).
        ///
        /// This timeout is applied to each individual connection attempt during both
        /// initial connection and reconnection operations.
        #[builder(default = DEFAULT_TIMEOUT)]
        timeout: Duration,

        /// Time to live for tcp packets (TCP only).
        #[builder(default = DEFAULT_TCP_TTL)]
        tcp_ttl: Duration,
    ) -> Result<Self, GraphiteError> {
        let address = address.into();
        let sock_addr = SocketAddr::new(IpAddr::from_str(&address)?, port);

        let connection = match protocol {
            Protocol::Tcp => {
                let tcp_stream = TcpStream::connect_timeout(&sock_addr, timeout)?;
                tcp_stream.set_ttl(tcp_ttl.as_secs() as u32)?;
                tcp_stream.set_nodelay(true)?;
                Connection::Tcp(tcp_stream)
            }
            Protocol::Udp => {
                let udp_socket = UdpSocket::bind("0.0.0.0:0")?;
                udp_socket.connect(sock_addr)?;
                Connection::Udp(udp_socket)
            }
        };

        Ok(Self {
            connection,
            sock_addr,
            _address: address,
            _port: port,
            protocol,
            retries,
            timeout,
            tcp_ttl,
        })
    }

    /// Attempts to reestablish the TCP connection to the Graphite server.
    ///
    /// This method tries to create a new connection up to `retries` times, replacing the
    /// existing connection if successful. It's called automatically by `send_message` when
    /// a send operation fails, but can also be called manually.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if reconnection succeeds, or `Err(GraphiteError)` if all retry
    /// attempts are exhausted.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use graphyne::{GraphiteClient, GraphiteMessage};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = GraphiteClient::builder()
    ///     .address("127.0.0.1")
    ///     .port(2003)
    ///     .build()?;
    ///
    /// // Manually reconnect if needed
    /// client.reconnect()?;
    ///
    /// let msg = GraphiteMessage::new("test.metric", "1");
    /// client.send_message(&msg)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn reconnect(&mut self) -> Result<(), GraphiteError> {
        // UDP doesn't need reconnection
        if self.protocol == Protocol::Udp {
            return Ok(());
        }

        let mut last_err: Error = Error::last_os_error();
        let mut i = 0;
        while i < self.retries {
            let connect = TcpStream::connect_timeout(&self.sock_addr, self.timeout);
            match connect {
                Ok(tcp) => {
                    tcp.set_ttl(self.tcp_ttl.as_secs() as u32)?;
                    tcp.set_nodelay(true)?;
                    self.connection = Connection::Tcp(tcp);
                    return Ok(());
                }
                Err(err) => last_err = err,
            }
            i += 1;
        }
        Err(GraphiteError {
            msg: format!("Graphite Error: {last_err}"),
        })
    }

    /// Sends a metric message to the Graphite server.
    ///
    /// This method writes the formatted metric to the connection. Behavior differs by protocol:
    ///
    /// **TCP**: If the write fails (e.g., due to a broken connection), it automatically attempts
    /// to reconnect and retry the send operation up to `retries` times.
    ///
    /// **UDP**: Sends the metric as a datagram. No retry logic is applied; if the send fails,
    /// an error is returned immediately.
    ///
    /// # Arguments
    ///
    /// * `msg` - A reference to the `GraphiteMessage` to send
    ///
    /// # Returns
    ///
    /// Returns `Ok(usize)` with the number of bytes written if successful, or
    /// `Err(GraphiteError)` if all retry attempts fail (TCP) or the send fails (UDP).
    ///
    /// # Examples
    ///
    /// ## TCP: Single metric with retry
    ///
    /// ```rust,no_run
    /// use graphyne::{GraphiteClient, GraphiteMessage, Protocol};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = GraphiteClient::builder()
    ///     .address("127.0.0.1")
    ///     .port(2003)
    ///     .protocol(Protocol::Tcp)
    ///     .build()?;
    ///
    /// let msg = GraphiteMessage::new("cpu.usage", "45.2");
    /// let bytes_sent = client.send_message(&msg)?;
    /// println!("Sent {} bytes", bytes_sent);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## UDP: High-throughput metrics
    ///
    /// ```rust,no_run
    /// use graphyne::{GraphiteClient, GraphiteMessage, Protocol};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = GraphiteClient::builder()
    ///     .address("127.0.0.1")
    ///     .port(2003)
    ///     .protocol(Protocol::Udp)
    ///     .build()?;
    ///
    /// let msg = GraphiteMessage::new("requests.count", "1000");
    /// client.send_message(&msg)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_message(&mut self, msg: &GraphiteMessage) -> Result<usize, GraphiteError> {
        let data = msg.to_string();

        match &mut self.connection {
            Connection::Udp(udp_socket) => {
                udp_socket.send(data.as_bytes())?;
                Ok(data.len())
            }
            Connection::Tcp(_) => {
                let mut last_err: Error = Error::last_os_error();
                // Handle TCP with retries
                for _ in 0..self.retries {
                    if let Connection::Tcp(tcp_stream) = &mut self.connection {
                        let res = tcp_stream.write_all(data.as_bytes());
                        match res {
                            Ok(_) => return Ok(data.len()),
                            Err(err) => {
                                last_err = err;
                                // Reconnect after error
                                self.reconnect()?;
                            }
                        }
                    }
                }

                Err(GraphiteError {
                    msg: format!("Graphite Error: {last_err}"),
                })
            }
        }
    }

    /// Sends multiple metric messages in a single operation.
    ///
    /// This method combines multiple messages and sends them together, which can be more
    /// efficient than sending messages individually.
    ///
    /// **TCP**: Messages are concatenated and sent over the TCP stream with retry logic.
    ///
    /// **UDP**: Messages are concatenated and sent as a single datagram. Note that large
    /// batches may exceed UDP packet size limits (typically ~64KB).
    ///
    /// # Arguments
    ///
    /// * `msgs` - A slice of `GraphiteMessage` references to send
    ///
    /// # Returns
    ///
    /// Returns `Ok(usize)` with the total number of bytes written if successful, or
    /// `Err(GraphiteError)` if the operation fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use graphyne::{GraphiteClient, GraphiteMessage};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = GraphiteClient::builder()
    ///     .address("127.0.0.1")
    ///     .port(2003)
    ///     .build()?;
    ///
    /// let metrics = vec![
    ///     GraphiteMessage::new("server1.cpu", "45"),
    ///     GraphiteMessage::new("server1.memory", "80"),
    ///     GraphiteMessage::new("server1.disk", "65"),
    /// ];
    ///
    /// client.send_batch_message(&metrics)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_batch_message(&mut self, msgs: &[GraphiteMessage]) -> Result<usize, GraphiteError> {
        match &mut self.connection {
            Connection::Udp(udp_socket) => {
                /// Maximum total payload size per UDP message.
                const MAX_UDP_SIZE: usize = 1400;
                let mut total_sent_bytes = 0;
                let mut current_batch = String::with_capacity(MAX_UDP_SIZE);

                for msg in msgs {
                    let msg_str = msg.to_string();
                    if !current_batch.is_empty()
                        && current_batch.len() + msg_str.len() > MAX_UDP_SIZE
                    {
                        udp_socket.send(current_batch.as_bytes())?;
                        total_sent_bytes += current_batch.len();
                        current_batch.clear();
                    }

                    // If the size of the message is larger than 1400 bytes, we need to send it
                    // on its own with the understanding that it may be truncated during transmit.
                    if msg_str.len() > MAX_UDP_SIZE {
                        udp_socket.send(msg_str.as_bytes())?;
                        total_sent_bytes += msg_str.len();
                    } else {
                        current_batch.push_str(&msg_str);
                    }
                }

                if !current_batch.is_empty() {
                    udp_socket.send(current_batch.as_bytes())?;
                    total_sent_bytes += current_batch.len();
                }

                Ok(total_sent_bytes)
            }
            Connection::Tcp(_) => {
                let combined: String = msgs.iter().map(ToString::to_string).collect();
                let mut last_err: Error = Error::last_os_error();
                // Handle TCP with retries
                for _ in 0..self.retries {
                    if let Connection::Tcp(tcp_stream) = &mut self.connection {
                        let res = tcp_stream.write_all(combined.as_bytes());
                        match res {
                            Ok(_) => return Ok(combined.len()),
                            Err(err) => {
                                last_err = err;
                                // Reconnect after error
                                self.reconnect()?;
                            }
                        }
                    }
                }

                Err(GraphiteError {
                    msg: format!("Graphite Error: {last_err}"),
                })
            }
        }
    }
}

impl Drop for GraphiteClient {
    /// Any errors during shutdown are silently ignored.
    fn drop(&mut self) {
        if let Connection::Tcp(tcp_stream) = &self.connection {
            let _ = tcp_stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// A metric message to be sent to Graphite.
///
/// `GraphiteMessage` represents a single metric data point in the Graphite plaintext protocol
/// format. It consists of a metric path, a value, and a Unix timestamp.
///
/// # Format
///
/// Messages are formatted as: `metric.path value timestamp\n`
///
/// For example: `servers.web01.cpu.usage 45.2 1609459200\n`
///
/// # Timestamps
///
/// Timestamps are automatically generated at message creation time using the system clock.
/// They represent seconds since the Unix epoch (January 1, 1970 00:00:00 UTC).
///
/// # Metric Paths
///
/// Metric paths should follow Graphite's hierarchical naming conventions using dots as
/// separators. Common patterns include:
///
/// - `company.application.server.component.metric`
/// - `environment.service.host.subsystem.value`
///
/// # Examples
///
/// ```rust
/// use graphyne::GraphiteMessage;
///
/// // Create a message for CPU usage
/// let cpu_msg = GraphiteMessage::new("servers.web01.cpu.usage", "45.2");
///
/// // Create a message for request count
/// let req_msg = GraphiteMessage::new("app.requests.total", "1000");
///
/// // Create a message with namespace hierarchy
/// let metric = GraphiteMessage::new("prod.api.gateway.latency.p95", "125");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GraphiteMessage {
    /// The hierarchical path identifying this metric in Graphite.
    ///
    /// Should use dots as separators (e.g., "servers.web01.cpu.usage").
    metric_path: String,

    /// The numeric value of the metric as a string.
    ///
    /// While Graphite expects numeric values, this is stored as a string to avoid
    /// unnecessary conversions and to preserve the exact formatting provided by the caller.
    value: String,

    /// Unix timestamp (seconds since epoch) when this message was created.
    ///
    /// Generated automatically at construction time using `SystemTime::now()`.
    timestamp: u64,
}

impl GraphiteMessage {
    /// Creates a new metric message with the current timestamp.
    ///
    /// The timestamp is automatically generated using the system clock at the moment
    /// this method is called.
    ///
    /// # Arguments
    ///
    /// * `metric_path` - The hierarchical path for this metric (e.g., "app.cpu.usage")
    /// * `value` - The metric value as a string (e.g., "42" or "3.14")
    ///
    /// # Examples
    ///
    /// ```rust
    /// use graphyne::GraphiteMessage;
    ///
    /// // Integer value
    /// let count = GraphiteMessage::new("requests.count", "150");
    ///
    /// // Floating point value
    /// let temp = GraphiteMessage::new("sensors.temperature", "23.5");
    ///
    /// // Large value
    /// let bytes = GraphiteMessage::new("network.bytes.sent", "1048576");
    /// ```
    pub fn new(metric_path: &str, value: &str) -> Self {
        Self {
            metric_path: metric_path.to_string(),
            value: value.to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Creates a new metric message with the provided timestamp.
    ///
    ///
    /// # Arguments
    ///
    /// * `metric_path` - The hierarchical path for this metric (e.g., "app.cpu.usage")
    /// * `value` - The metric value as a string (e.g., "42" or "3.14")
    /// * `ts` - The metric timestamp as a unix epoch.
    /// # Examples
    ///
    /// ```rust
    /// use graphyne::GraphiteMessage;
    ///
    /// // Integer value
    /// let count = GraphiteMessage::new_with_ts("requests.count", "150", 1768405993);
    ///
    /// // Floating point value
    /// let temp = GraphiteMessage::new_with_ts("sensors.temperature", "23.5", 1768405993);
    ///
    /// // Large value
    /// let bytes = GraphiteMessage::new_with_ts("network.bytes.sent", "1048576", 1768405993);
    /// ```
    pub fn new_with_ts(metric_path: &str, value: &str, ts: u64) -> Self {
        Self {
            metric_path: metric_path.to_string(),
            value: value.to_string(),
            timestamp: ts,
        }
    }
}

impl fmt::Display for GraphiteMessage {
    /// Formats the message according to the Graphite plaintext protocol.
    ///
    /// The output format is: `metric_path value timestamp\n`
    ///
    /// This format is used when sending messages to the Graphite server.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use graphyne::GraphiteMessage;
    ///
    /// let msg = GraphiteMessage::new("test.metric", "42");
    /// let formatted = msg.to_string();
    /// // Output: "test.metric 42 1609459200\n" (timestamp will vary)
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} {} {}", self.metric_path, self.value, self.timestamp)
    }
}

/// Error type for Graphite client operations.
///
/// `GraphiteError` wraps various error conditions that can occur during client operations,
/// including connection failures, send failures, and address parsing errors.
///
/// # Examples
///
/// ```rust
/// use graphyne::{GraphiteClient, GraphiteError};
/// use std::time::Duration;
///
/// fn try_connect() -> Result<GraphiteClient, GraphiteError> {
///     GraphiteClient::builder()
///         .address("127.0.0.1")
///         .port(2003)
///         .timeout(Duration::from_millis(100))
///         .build()
/// }
///
/// match try_connect() {
///     Ok(client) => println!("Connected successfully"),
///     Err(e) => eprintln!("Connection failed: {}", e),
/// }
/// ```
#[derive(Clone)]
pub struct GraphiteError {
    /// Human-readable error message describing what went wrong.
    pub msg: String,
}

impl fmt::Display for GraphiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl fmt::Debug for GraphiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GraphiteError {{ msg: {:?} }}", self.msg)
    }
}

impl std::error::Error for GraphiteError {}

impl From<AddrParseError> for GraphiteError {
    /// Converts address parsing errors into `GraphiteError`.
    ///
    /// This is called when the provided address string cannot be parsed as a valid IP address.
    fn from(err: AddrParseError) -> Self {
        GraphiteError {
            msg: err.to_string(),
        }
    }
}

impl From<Error> for GraphiteError {
    /// Converts I/O errors into `GraphiteError`.
    ///
    /// This handles connection errors, timeout errors, and write failures.
    fn from(err: Error) -> Self {
        GraphiteError {
            msg: err.to_string(),
        }
    }
}
