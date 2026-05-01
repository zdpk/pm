//! Control plane HTTP for the daemon.
//!
//! Endpoints (all on `127.0.0.1:dev.control_port`, default 7101):
//!
//! | method | path     | purpose                                            |
//! |--------|----------|----------------------------------------------------|
//! | GET    | /health  | liveness — returns 200 with `{ pid }`              |
//! | GET    | /status  | introspection — pid, uptime, route count, port     |
//! | POST   | /reload  | force-reload routes.json (debugging aid)           |
//! | POST   | /stop    | trigger graceful shutdown                          |
//!
//! All responses use `application/json`. Bodies are best-effort; clients
//! that only care about status codes (e.g. `pm proxy status` printing a
//! tabular view) should still work if the body parse fails.

use crate::commands::proxy::daemon;
use crate::config::{daemon_pid_path, load_config};
use crate::routes::load_routes;
use anyhow::{Context, Result};
use bytes::Bytes;
use colored::Colorize;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::Notify;

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusBody {
    pub pid: u32,
    pub uptime_sec: u64,
    pub proxy_port: u16,
    pub control_port: u16,
    pub routes_count: usize,
}

pub async fn serve(port: u16, shutdown: Arc<Notify>) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    let started_at = Instant::now();
    eprintln!("pm-daemon: control plane on http://{addr}");

    loop {
        let (stream, _peer) = tokio::select! {
            accept = listener.accept() => match accept {
                Ok(s) => s,
                Err(e) => { eprintln!("pm-daemon: control accept error: {e}"); continue; }
            },
            _ = shutdown.notified() => {
                eprintln!("pm-daemon: control plane shutting down");
                break;
            }
        };

        let shutdown = shutdown.clone();
        let io = TokioIo::new(stream);
        tokio::spawn(async move {
            let svc = service_fn(move |req: Request<Incoming>| {
                let shutdown = shutdown.clone();
                async move {
                    Ok::<_, Infallible>(dispatch(req, started_at, shutdown).await)
                }
            });
            if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                let _ = e;
            }
        });
    }
    Ok(())
}

async fn dispatch(
    req: Request<Incoming>,
    started_at: Instant,
    shutdown: Arc<Notify>,
) -> Response<Full<Bytes>> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/health") => json(StatusCode::OK, &serde_json::json!({ "pid": std::process::id() })),
        (&Method::GET, "/status") => match build_status(started_at) {
            Ok(s) => json(StatusCode::OK, &s),
            Err(e) => text(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e}")),
        },
        (&Method::POST, "/reload") => {
            // The proxy already mtime-checks on every request, so explicit
            // reload is a no-op here. Provided for parity with portless and
            // user expectations.
            json(StatusCode::OK, &serde_json::json!({ "ok": true }))
        }
        (&Method::POST, "/stop") => {
            shutdown.notify_waiters();
            json(StatusCode::OK, &serde_json::json!({ "ok": true }))
        }
        _ => text(StatusCode::NOT_FOUND, "not found"),
    }
}

fn build_status(started_at: Instant) -> Result<StatusBody> {
    let config = load_config()?;
    let routes = load_routes().unwrap_or_default();
    Ok(StatusBody {
        pid: std::process::id(),
        uptime_sec: started_at.elapsed().as_secs(),
        proxy_port: config.dev.proxy_port,
        control_port: config.dev.control_port,
        routes_count: routes.entries.len(),
    })
}

fn json<T: Serialize>(status: StatusCode, body: &T) -> Response<Full<Bytes>> {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(bytes)))
        .unwrap()
}

fn text(status: StatusCode, msg: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(msg.to_string())))
        .unwrap()
}

// ── Sync client used by the regular CLI ──

/// Quick liveness check: `GET /health` on the control port. Returns true
/// when the response is 200. Uses a blocking 1s connect timeout so the
/// caller (e.g. `daemon::ensure_running`) does not stall.
pub fn ping() -> Result<bool> {
    let config = load_config().ok();
    let port = config.map(|c| c.dev.control_port).unwrap_or(7101);
    blocking_get(port, "/health").map(|s| s == StatusCode::OK)
}

pub fn fetch_status() -> Result<StatusBody> {
    let config = load_config()?;
    let port = config.dev.control_port;
    let body = blocking_get_body(port, "/status")?;
    let parsed: StatusBody = serde_json::from_slice(&body).context("decoding /status body")?;
    Ok(parsed)
}

pub fn send_stop() -> Result<()> {
    let config = load_config()?;
    let port = config.dev.control_port;
    blocking_post(port, "/stop")?;
    Ok(())
}

// ── Sync HTTP helpers ──
//
// We avoid pulling in a full HTTP client crate for a handful of one-shot
// loopback requests. These helpers manually speak HTTP/1.1 over a TCP socket
// with a short timeout. Sufficient for control-plane interactions.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};

fn blocking_get(port: u16, path: &str) -> Result<StatusCode> {
    let mut stream = match TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse()?,
        Duration::from_millis(500),
    ) {
        Ok(s) => s,
        Err(_) => return Ok(StatusCode::SERVICE_UNAVAILABLE),
    };
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_millis(500)))?;

    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes())?;
    let mut buf = Vec::with_capacity(256);
    let _ = stream.read_to_end(&mut buf);
    let _ = stream.shutdown(Shutdown::Both);
    Ok(parse_status(&buf).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
}

fn blocking_get_body(port: u16, path: &str) -> Result<Vec<u8>> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse()?,
        Duration::from_millis(500),
    )?;
    stream.set_read_timeout(Some(Duration::from_millis(1000)))?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes())?;
    let mut buf = Vec::with_capacity(1024);
    stream.read_to_end(&mut buf)?;
    Ok(extract_body(&buf))
}

fn blocking_post(port: u16, path: &str) -> Result<StatusCode> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse()?,
        Duration::from_millis(500),
    )?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes())?;
    let mut buf = Vec::with_capacity(256);
    let _ = stream.read_to_end(&mut buf);
    Ok(parse_status(&buf).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
}

fn parse_status(buf: &[u8]) -> Option<StatusCode> {
    // "HTTP/1.1 200 OK\r\n..."
    let prefix = std::str::from_utf8(buf.get(..15)?).ok()?;
    let mut parts = prefix.split_whitespace();
    let _http = parts.next()?;
    let code: u16 = parts.next()?.parse().ok()?;
    StatusCode::from_u16(code).ok()
}

fn extract_body(buf: &[u8]) -> Vec<u8> {
    if let Some(idx) = find_header_end(buf) {
        buf[idx + 4..].to_vec()
    } else {
        Vec::new()
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

// ── CLI subcommand handlers ──

pub fn cmd_status() -> Result<()> {
    match daemon::check_alive()? {
        None => {
            println!("{} daemon: not running", "—".dimmed());
            Ok(())
        }
        Some(pid) => match fetch_status() {
            Ok(s) => {
                println!(
                    "{} daemon running (pid {})",
                    "✓".green(),
                    pid
                );
                println!("  uptime:       {}s", s.uptime_sec);
                println!("  proxy:        http://127.0.0.1:{}", s.proxy_port);
                println!("  control:      http://127.0.0.1:{}", s.control_port);
                println!("  routes:       {}", s.routes_count);
                Ok(())
            }
            Err(e) => {
                println!(
                    "{} daemon pid {} alive but /status failed: {}",
                    "!".yellow(),
                    pid,
                    e
                );
                Ok(())
            }
        },
    }
}

pub fn cmd_stop() -> Result<()> {
    if daemon::check_alive()?.is_none() {
        println!("{} daemon: not running", "—".dimmed());
        return Ok(());
    }
    send_stop()?;
    // Wait briefly for the pid file to disappear, indicating clean exit.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(2000) {
        if !daemon_pid_path().exists() {
            println!("{} daemon stopped", "✓".green());
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // If the pid file is still present, force-clean: the daemon may have
    // crashed mid-shutdown.
    let _ = fs::remove_file(daemon_pid_path());
    println!(
        "{} daemon stop request sent (pid file cleared after timeout)",
        "!".yellow()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_ok() {
        let buf = b"HTTP/1.1 200 OK\r\n\r\n";
        assert_eq!(parse_status(buf), Some(StatusCode::OK));
    }

    #[test]
    fn parse_status_404() {
        let buf = b"HTTP/1.1 404 Not Found\r\n\r\n";
        assert_eq!(parse_status(buf), Some(StatusCode::NOT_FOUND));
    }

    #[test]
    fn extract_body_finds_separator() {
        let buf = b"HTTP/1.1 200 OK\r\nA: 1\r\n\r\nbody-content";
        assert_eq!(extract_body(buf), b"body-content".to_vec());
    }
}
