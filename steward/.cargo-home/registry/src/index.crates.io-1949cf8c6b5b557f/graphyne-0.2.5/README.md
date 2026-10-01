# 🔥 Graphyne

[![Crates.io](https://img.shields.io/crates/v/graphyne.svg)](https://crates.io/crates/graphyne)
[![Documentation](https://docs.rs/graphyne/badge.svg)](https://docs.rs/graphyne)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

A blazingly fast, ergonomic Rust client for sending metrics to [Graphite](https://graphiteapp.org/) Carbon daemons.

## ✨ Features

- **🎯 Simple API** - Intuitive builder pattern for easy configuration
- **🔄 Auto-reconnection** - Automatic retry logic with configurable attempts
- **⚡ Zero-copy writes** - Efficient metric transmission with minimal overhead
- **🛡️ Type-safe** - Leverages Rust's type system for compile-time guarantees
- **⏱️ Configurable timeouts** - Fine-grained control over connection behavior
- **📊 Real-time metrics** - Unix timestamp generation for accurate time-series data

## 🚀 Quick Start

Add Graphyne to your project:

```bash
cargo add graphyne
```

## 📖 Usage

### Basic Example

```rust
use graphyne::{GraphiteClient, GraphiteMessage};

// Create a client with default settings
let mut client = GraphiteClient::builder()
    .address("127.0.0.1")
    .port(2003)
    .build()?;

// Send a metric
let message = GraphiteMessage::new("app.requests.count", "42");
client.send_message(&message)?;
```

### Advanced Configuration

```rust
use graphyne::GraphiteClient;
use std::time::Duration;

let mut client = GraphiteClient::builder()
    .address("127.0.0.1")
    .port(2003)
    .retries(5)                       // Optional
    .timeout(Duration::from_secs(10)) // Optional
    .build()?;
```

### Sending Multiple Metrics

```rust
use graphyne::{GraphiteClient, GraphiteMessage};

let mut client = GraphiteClient::builder()
    .address("127.0.0.1")
    .port(2003)
    .build()?;

// Send various application metrics
let metrics = vec![
    GraphiteMessage::new("app.cpu.usage", "45.2"),
    GraphiteMessage::new("app.memory.bytes", "1048576"),
    GraphiteMessage::new("app.requests.latency", "125"),
];

for metric in &metrics {
    client.send_message(metric)?;
}
```

### Connection Behavior

- **Automatic reconnection**: If a send fails, the client automatically attempts to reconnect
- **Retry logic**: Configurable number of retry attempts for both connection and send operations
- **Graceful shutdown**: Connections are properly closed when the client is dropped

## ⚠️ Known Limitations

- **IP addresses only**: Currently only supports IP addresses, not DNS hostnames
- **TCP only**: Uses TCP plaintext protocol (port 2003 by default)
- **No UDP support**: UDP protocol is not yet supported
- **No batching**: Each message is sent individually

## 🤝 Contributing

We welcome contributions! Here's how you can help:

1. **Fork** the repository
2. **Create** a feature branch (`git checkout -b feature/amazing-feature`)
3. **Commit** your changes (`git commit -m 'Add amazing feature'`)
4. **Push** to the branch (`git push origin feature/amazing-feature`)
5. **Open** a Pull Request

### Development

```bash
# Run tests
cargo test

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy -- -D warnings
```

## 📜 License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

Built with ❤️ by the [Vultr](https://www.vultr.com/) Core Cloud Engineering team.

## 📚 Additional Resources

- [Graphite Documentation](https://graphite.readthedocs.io/)
- [Graphite Metric Naming Best Practices](https://graphite.readthedocs.io/en/latest/feeding-carbon.html)
- [Carbon Protocol Specification](https://graphite.readthedocs.io/en/latest/feeding-carbon.html#the-plaintext-protocol)

---

Made with 🦀 Rust | Maintained by Vultr
