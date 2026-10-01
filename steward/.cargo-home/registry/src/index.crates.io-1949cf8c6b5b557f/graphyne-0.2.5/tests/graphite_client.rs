#[cfg(test)]
mod tests {
    use graphyne::GraphiteClient;
    use std::net::TcpListener;
    use std::time::Duration;

    // Dummy listener that accepts connections
    struct DummyGraphiteServer {
        _handle: std::thread::JoinHandle<()>,
    }

    impl DummyGraphiteServer {
        fn start(port: u16) -> Self {
            let handle = std::thread::spawn(move || {
                let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).unwrap();
                while listener.accept().is_ok() {
                    // Just accept and drop the connection
                }
            });

            // pause to start -- seems to be a race without waiting
            std::thread::sleep(Duration::from_millis(50));

            Self { _handle: handle }
        }
    }

    #[test]
    fn test_client_builder_defaults() {
        let port = 20031;
        let _ = DummyGraphiteServer::start(port);

        let client = GraphiteClient::builder()
            .address("127.0.0.1")
            .port(port)
            .build()
            .unwrap();
        insta::with_settings!({filters => vec![
            (r"        addr: [\d.]+:\d+", "        addr: <EPHEMERAL>"),
            (r"        fd: \d+", "        fd: <EPHEMERAL>"),
        ]}, {
            insta::assert_debug_snapshot!(client);
        });
    }

    #[test]
    fn test_client_builder_custom_retries() {
        let port = 20032;
        let _ = DummyGraphiteServer::start(port);

        let client = GraphiteClient::builder()
            .address("127.0.0.1")
            .port(port)
            .retries(10)
            .build()
            .unwrap();

        insta::with_settings!({filters => vec![
            (r"        addr: [\d.]+:\d+", "        addr: <EPHEMERAL>"),
            (r"        fd: \d+", "        fd: <EPHEMERAL>"),
        ]}, {
            insta::assert_debug_snapshot!(client);
        });
    }

    #[test]
    fn test_client_builder_custom_timeout() {
        let port = 20033;
        let _ = DummyGraphiteServer::start(port);

        let client = GraphiteClient::builder()
            .address("127.0.0.1")
            .port(port)
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();

        insta::with_settings!({filters => vec![
            (r"        addr: [\d.]+:\d+", "        addr: <EPHEMERAL>"),
            (r"        fd: \d+", "        fd: <EPHEMERAL>"),
        ]}, {
            insta::assert_debug_snapshot!(client);
        });
    }

    #[test]
    fn test_client_builder_all_options() {
        let port = 20034;
        let _ = DummyGraphiteServer::start(port);

        let client = GraphiteClient::builder()
            .address("127.0.0.1")
            .port(port)
            .retries(7)
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();

        insta::with_settings!({filters => vec![
            (r"        addr: [\d.]+:\d+", "        addr: <EPHEMERAL>"),
            (r"        fd: \d+", "        fd: <EPHEMERAL>"),
        ]}, {
            insta::assert_debug_snapshot!(client);
        });
    }

    #[test]
    fn test_connection_failure() {
        let result = GraphiteClient::builder()
            .address("127.0.0.1")
            .port(6969)
            .timeout(Duration::from_millis(100))
            .build();

        assert!(result.is_err());
    }
}
