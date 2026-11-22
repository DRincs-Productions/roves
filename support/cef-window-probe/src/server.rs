//! Loopback origin for the game directory: `http://127.0.0.1:<port>/`, bound to the IPv4
//! loopback interface only. tower-http's `ServeDir` does the file serving (rejects `..`
//! traversal, sets MIME types, answers Range requests for media); this module only owns the
//! listener, the background runtime and the shutdown.

use std::{
    io,
    net::{Ipv4Addr, TcpListener},
    path::Path,
    thread::JoinHandle,
};

use axum::Router;
use tokio::sync::oneshot;
use tower_http::services::ServeDir;

pub struct Server {
    pub origin: String,
    shutdown: oneshot::Sender<()>,
    thread: JoinHandle<io::Result<()>>,
}

/// Binds synchronously (so the origin is known before CEF starts) and serves on a dedicated
/// thread. Port 0 picks a free port, giving every run its own origin.
pub fn start(root: &Path, port: u16) -> io::Result<Server> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    listener.set_nonblocking(true)?;
    let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let app = Router::new().fallback_service(ServeDir::new(root));
    let (shutdown, stopped) = oneshot::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("roves-cef-loopback".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()?;
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await
            })
        })?;
    Ok(Server {
        origin,
        shutdown,
        thread,
    })
}

impl Server {
    /// Stops accepting, lets in-flight responses finish and joins the thread.
    pub fn stop(self) -> io::Result<()> {
        let _ = self.shutdown.send(());
        self.thread
            .join()
            .map_err(|_| io::Error::other("loopback server thread panicked"))?
    }
}
