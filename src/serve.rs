//! Shutdown for the HTTP servers (Agent API, MCP, AI model API).
//!
//! `axum::serve` on its own runs until the process is killed: a request in
//! flight when Ctrl-C arrives is cut off mid-response. `serve_until` stops
//! accepting on the signal, lets open requests finish, and gives up on the
//! ones still open after `drain` (an SSE stream or a long eval would
//! otherwise hold the process up for as long as the client keeps it open).

use std::future::Future;
use std::time::Duration;

use axum::Router;
use tokio::net::TcpListener;

/// How long open connections get to finish after the shutdown signal.
pub const DRAIN: Duration = Duration::from_secs(10);

/// Serve `app` until `signal` resolves, then drain for at most `drain`.
pub async fn serve_until<F>(
    listener: TcpListener,
    app: Router,
    signal: F,
    drain: Duration,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let (fired_tx, fired_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        signal.await;
        let _ = fired_tx.send(());
    });
    let deadline = async move {
        // An Err means the server returned before any signal; never fires then.
        if fired_rx.await.is_ok() {
            tokio::time::sleep(drain).await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    tokio::select! {
        r = server => r,
        _ = deadline => {
            eprintln!("shutdown: connections still open after {}s were closed", drain.as_secs());
            Ok(())
        }
    }
}

/// Resolves on Ctrl-C, or on SIGTERM on Unix.
///
/// A handler that cannot be installed never fires, rather than firing at once
/// and stopping a server that was never asked to stop.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = term => {}
    }
    eprintln!("shutdown: signal received, finishing open requests");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn request(addr: std::net::SocketAddr, path: &str) -> String {
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let req = format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out).await;
        out
    }

    fn app() -> Router {
        Router::new()
            .route("/fast", get(|| async { "fast" }))
            .route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    "slow"
                }),
            )
            .route(
                "/hang",
                get(|| async {
                    tokio::time::sleep(Duration::from_secs(600)).await;
                    "never"
                }),
            )
    }

    #[tokio::test]
    async fn returns_when_signalled_with_nothing_open() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve_until(
            listener,
            app(),
            async move {
                let _ = rx.await;
            },
            Duration::from_secs(5),
        ));
        assert!(request(addr, "/fast").await.ends_with("fast"));
        tx.send(()).unwrap();
        let r = tokio::time::timeout(Duration::from_secs(3), server).await;
        assert!(r.expect("server did not stop").unwrap().is_ok());
    }

    #[tokio::test]
    async fn an_open_request_finishes_after_the_signal() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve_until(
            listener,
            app(),
            async move {
                let _ = rx.await;
            },
            Duration::from_secs(5),
        ));
        let slow = tokio::spawn(request(addr, "/slow"));
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx.send(()).unwrap();
        let body = slow.await.unwrap();
        assert!(
            body.ends_with("slow"),
            "in-flight request was cut off: {body:?}"
        );
        let r = tokio::time::timeout(Duration::from_secs(3), server).await;
        assert!(r.expect("server did not stop").unwrap().is_ok());
    }

    #[tokio::test]
    async fn a_hung_request_does_not_outlast_the_drain() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve_until(
            listener,
            app(),
            async move {
                let _ = rx.await;
            },
            Duration::from_millis(300),
        ));
        let _hang = tokio::spawn(request(addr, "/hang"));
        tokio::time::sleep(Duration::from_millis(100)).await;
        let t0 = Instant::now();
        tx.send(()).unwrap();
        let r = tokio::time::timeout(Duration::from_secs(5), server).await;
        assert!(r.expect("drain did not end").unwrap().is_ok());
        assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
    }

    #[tokio::test]
    async fn no_new_connections_after_the_signal() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve_until(
            listener,
            app(),
            async move {
                let _ = rx.await;
            },
            Duration::from_secs(5),
        ));
        assert!(request(addr, "/fast").await.ends_with("fast"));
        tx.send(()).unwrap();
        let r = tokio::time::timeout(Duration::from_secs(3), server).await;
        assert!(r.expect("server did not stop").unwrap().is_ok());
        // Windows takes about 2s to refuse a connection to a closed local
        // port, so no answer within the timeout also means not served.
        let late = tokio::time::timeout(Duration::from_secs(2), async {
            match tokio::net::TcpStream::connect(addr).await {
                Err(_) => true,
                Ok(mut s) => {
                    let _ = s.write_all(b"GET /fast HTTP/1.1\r\nHost: x\r\n\r\n").await;
                    let mut out = String::new();
                    let _ = s.read_to_string(&mut out).await;
                    !out.contains("fast")
                }
            }
        })
        .await
        .unwrap_or(true);
        assert!(late, "a connection after shutdown was served");
    }
}
