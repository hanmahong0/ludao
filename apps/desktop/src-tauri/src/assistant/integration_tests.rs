#[cfg(all(test, unix))]
mod live {
    use super::super::sidecar::spawn_daemon;
    use super::super::zeroclaw_rpc::RpcClient;
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    #[ignore = "requires verified LUDAO_ZEROCLAW_BIN"]
    fn product_initialize_handshake() {
        let data_dir = std::env::temp_dir().join(format!("ludao-zc-it-{}", Uuid::new_v4()));
        let socket = data_dir.join("daemon.sock");
        let mut handle = spawn_daemon(&data_dir, &socket).expect("spawn");
        let mut client =
            RpcClient::connect(&socket, Duration::from_secs(10)).expect("connect");
        let init = client.initialize().expect("initialize");
        assert_eq!(init.server_version.as_deref(), Some("0.9.0"));
        handle.stop();
    }
}