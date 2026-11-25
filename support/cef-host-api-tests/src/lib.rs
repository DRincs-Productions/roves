//! See Cargo.toml: the players' loopback server and host API under plain `cargo test`, plus
//! the macOS/Linux key-code rules (`cef-macos-gate/src/key_rules.rs`) and the Windows
//! player's pure rules they build on (`cef-netrender-probe/src/game_rules.rs`).

#[allow(dead_code)]
#[path = "../../cef-window-probe/src/server.rs"]
mod server;

#[allow(dead_code)]
#[path = "../../cef-window-probe/src/host_api.rs"]
mod host_api;

#[allow(dead_code)]
#[path = "../../cef-netrender-probe/src/game_rules.rs"]
mod game_rules;

#[allow(dead_code)]
#[path = "../../cef-macos-gate/src/key_rules.rs"]
mod key_rules;

#[cfg(test)]
mod end_to_end {
    //! The host API mounted on the real loopback server, raw HTTP/1.1 over TCP.
    use std::{
        io::{Read, Write},
        net::TcpStream,
        path::PathBuf,
        sync::Arc,
    };

    use crate::{host_api, server};

    fn request(addr: &str, head: &str, body: &[u8]) -> (u16, String) {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .write_all(
                format!(
                    "{head}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
        stream.write_all(body).unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text.split(' ').nth(1).unwrap().parse().unwrap();
        (status, text)
    }

    #[test]
    fn saves_survive_a_restart_and_foreign_hosts_are_refused() {
        let fixture = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../cef-window-probe/fixtures/static-compat"
        ));
        let profile =
            std::env::temp_dir().join(format!("roves-host-api-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        let start = || {
            let (api, saves) = host_api::HostApi::new(host_api::HostConfig {
                saves_dir: Some(profile.join("saves")),
                engine_version: "e2e".into(),
                request_exit: Arc::new(|_| {}),
                steam: None,
            });
            assert!(saves.is_ok());
            server::start_with(&fixture, 0, |origin| api.router(origin)).unwrap()
        };
        let first = start();
        let addr = first.origin.trim_start_matches("http://").to_owned();
        let (status, text) = request(
            &addr,
            &format!(
                "POST /__roves/saves/write?key=slot-1 HTTP/1.1\r\nHost: {addr}\r\n\
                 X-Roves-Api: 1\r\nOrigin: {}\r\n",
                first.origin
            ),
            b"{\"chapter\":3}",
        );
        assert_eq!(status, 200, "{text}");
        assert!(text.ends_with("true"), "{text}");
        // Game files are still served next to the API.
        let (status, _) = request(&addr, &format!("GET / HTTP/1.1\r\nHost: {addr}\r\n"), b"");
        assert_eq!(status, 200);
        // DNS rebinding (foreign Host), a missing header and a preflight are refused.
        for head in [
            "GET /__roves/saves/list HTTP/1.1\r\nHost: rebound.example\r\nX-Roves-Api: 1\r\n"
                .to_owned(),
            format!("GET /__roves/saves/list HTTP/1.1\r\nHost: {addr}\r\n"),
            format!(
                "OPTIONS /__roves/saves/list HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://evil.example\r\n\
                 Access-Control-Request-Method: POST\r\n"
            ),
        ] {
            let (status, text) = request(&addr, &head, b"");
            assert_eq!(status, 403, "{head}");
            assert!(
                !text.to_ascii_lowercase().contains("access-control-"),
                "{text}"
            );
        }
        first.stop().unwrap();

        let second = start();
        let addr = second.origin.trim_start_matches("http://").to_owned();
        let (status, text) = request(
            &addr,
            &format!(
                "GET /__roves/saves/read?key=slot-1 HTTP/1.1\r\nHost: {addr}\r\nX-Roves-Api: 1\r\n"
            ),
            b"",
        );
        assert_eq!(status, 200, "{text}");
        // base64 of {"chapter":3}
        assert!(text.ends_with("\"eyJjaGFwdGVyIjozfQ==\""), "{text}");
        second.stop().unwrap();
        let _ = std::fs::remove_dir_all(&profile);
    }
}
