//! Reverse-proxy HTTP server.
//!
//! Reads `routes.json` (lazily, mtime-cached) and forwards incoming requests
//! to the upstream port matching the `Host` header. Returns 404 for unknown
//! hostnames.

use crate::config::routes_path;
use crate::routes::{RoutesData, load_routes};
use anyhow::Result;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::net::TcpListener;
use tokio::sync::{Notify, RwLock};

pub async fn serve(port: u16, shutdown: Arc<Notify>) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    let cache = Arc::new(RwLock::new(RoutesCache::new()));
    eprintln!("pm-daemon: proxy listening on http://{addr}");

    loop {
        let (stream, _peer) = tokio::select! {
            accept = listener.accept() => match accept {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("pm-daemon: accept error: {e}");
                    continue;
                }
            },
            _ = shutdown.notified() => {
                eprintln!("pm-daemon: proxy shutting down");
                break;
            }
        };

        let cache = cache.clone();
        let io = TokioIo::new(stream);
        tokio::spawn(async move {
            let svc = service_fn(move |req: Request<Incoming>| {
                let cache = cache.clone();
                async move { Ok::<_, Infallible>(handle(req, cache).await) }
            });
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, svc)
                .with_upgrades()
                .await
            {
                // Connection errors are common (client disconnects); log only.
                let msg = e.to_string();
                if !msg.contains("incomplete message") {
                    eprintln!("pm-daemon: connection error: {msg}");
                }
            }
        });
    }
    Ok(())
}

async fn handle(req: Request<Incoming>, cache: Arc<RwLock<RoutesCache>>) -> Response<Full<Bytes>> {
    let host_header = req
        .headers()
        .get(hyper::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.to_string());

    let host = match host_header {
        Some(h) => strip_port(&h),
        None => {
            return error(StatusCode::BAD_REQUEST, "Missing Host header");
        }
    };

    let upstream_port = {
        let mut guard = cache.write().await;
        guard.refresh_if_changed();
        guard.lookup(&host)
    };

    let upstream_port = match upstream_port {
        Some(p) => p,
        None => {
            return error(
                StatusCode::NOT_FOUND,
                &format!("No pm route for hostname '{host}'"),
            );
        }
    };

    match forward(req, upstream_port).await {
        Ok(resp) => resp,
        Err(e) => error(
            StatusCode::BAD_GATEWAY,
            &format!("upstream error on port {upstream_port}: {e}"),
        ),
    }
}

async fn forward(
    req: Request<Incoming>,
    upstream_port: u16,
) -> Result<Response<Full<Bytes>>> {
    use hyper::client::conn::http1::handshake;

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", upstream_port)).await?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = handshake(io).await?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            // Upstream HTTP framing errors are usually transient.
            let _ = e;
        }
    });

    // Strip hop-by-hop headers per RFC 7230 §6.1
    let (mut parts, body) = req.into_parts();
    parts.headers.remove(hyper::header::CONNECTION);
    parts.headers.remove("proxy-connection");
    parts.headers.remove(hyper::header::TRANSFER_ENCODING);
    parts.headers.remove(hyper::header::UPGRADE);
    parts.headers.remove("keep-alive");
    let outgoing = Request::from_parts(parts, body);

    let resp = sender.send_request(outgoing).await?;
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await?.to_bytes();
    Ok(Response::from_parts(parts, Full::new(bytes)))
}

fn strip_port(host: &str) -> String {
    match host.rsplit_once(':') {
        Some((h, _port)) => h.to_string(),
        None => host.to_string(),
    }
}

fn error(status: StatusCode, msg: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(format!("{}\n", msg))))
        .unwrap()
}

// ── routes.json mtime-cached lookup ──

struct RoutesCache {
    data: RoutesData,
    mtime: Option<SystemTime>,
}

impl RoutesCache {
    fn new() -> Self {
        Self {
            data: RoutesData::default(),
            mtime: None,
        }
    }

    fn refresh_if_changed(&mut self) {
        let path = routes_path();
        let cur_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if cur_mtime != self.mtime {
            match load_routes() {
                Ok(d) => {
                    self.data = d;
                    self.mtime = cur_mtime;
                }
                Err(e) => {
                    // Tolerate mid-write states; keep previous cache.
                    eprintln!("pm-daemon: routes reload error (ignored): {e}");
                }
            }
        }
    }

    fn lookup(&self, hostname: &str) -> Option<u16> {
        self.data
            .entries
            .iter()
            .find(|e| e.hostname.eq_ignore_ascii_case(hostname))
            .map(|e| e.upstream_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_port_with_port() {
        assert_eq!(strip_port("api.work.localhost:7100"), "api.work.localhost");
    }

    #[test]
    fn strip_port_without_port() {
        assert_eq!(strip_port("api.work.localhost"), "api.work.localhost");
    }
}
