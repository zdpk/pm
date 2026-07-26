//! Read-only HTML dashboard for services managed by the PM orchestrator.
//!
//! `services.json` is the process registry. `ports.json` classifies each
//! service, and `routes.json` is joined only to find a browser URL, so canonical
//! and short aliases never create duplicate dashboard entries.

use crate::foreground_processes::{self, ForegroundProcessesData};
use crate::models::{PortKind, PortsData};
use crate::routes::{RouteEntry, RoutesData};
use crate::services::{self, ServicesData};
use anyhow::Context;
use bytes::Bytes;
use http_body_util::Full;
use hyper::header::{ALLOW, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE};
use hyper::{Method, Response, StatusCode};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(super) const HOSTNAME: &str = "pm.localhost";

const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; \
     form-action 'none'; frame-ancestors 'none'";
const TAILSCALE_CACHE_TTL: Duration = Duration::from_secs(10);
const TAILSCALE_COMMAND_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppStatus {
    Running,
    RouteMissing,
    RouteMismatch,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessMode {
    Background,
    Foreground,
}

impl ProcessMode {
    fn label(self) -> &'static str {
        match self {
            Self::Background => "Background",
            Self::Foreground => "Foreground",
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Foreground => "foreground",
        }
    }
}

impl AppStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::RouteMissing => "Route missing",
            Self::RouteMismatch => "Route mismatch",
            Self::Stale => "Process stopped",
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::RouteMissing | Self::RouteMismatch => "warning",
            Self::Stale => "stale",
        }
    }

    fn is_process_running(self) -> bool {
        self != Self::Stale
    }

    fn has_healthy_route(self) -> bool {
        self == Self::Running
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DashboardApp {
    workspace: String,
    project: String,
    service: String,
    kind: Option<PortKind>,
    mode: ProcessMode,
    pid: u32,
    upstream_port: Option<u16>,
    started_at: String,
    url: Option<String>,
    tailnet_urls: Vec<String>,
    status: AppStatus,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct DashboardSnapshot {
    apps: Vec<DashboardApp>,
    background_count: usize,
    foreground_count: usize,
    attention_count: usize,
}

struct DashboardSources<'a> {
    routes: &'a RoutesData,
    services: &'a ServicesData,
    ports: &'a PortsData,
    foreground: &'a ForegroundProcessesData,
    tailnet_links: &'a HashMap<u16, Vec<String>>,
}

pub(super) fn is_dashboard_host(host: &str, configured_hosts: &[String]) -> bool {
    host.eq_ignore_ascii_case(HOSTNAME)
        || host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || configured_hosts
            .iter()
            .any(|configured| host.eq_ignore_ascii_case(configured.trim_end_matches('.')))
}

pub(super) async fn response(
    method: &Method,
    path: &str,
    routes: &RoutesData,
    proxy_port: u16,
) -> Response<Full<Bytes>> {
    if path != "/" {
        return plain_response(
            method,
            StatusCode::NOT_FOUND,
            "Dashboard page not found",
            None,
        );
    }
    if method != Method::GET && method != Method::HEAD {
        return plain_response(
            method,
            StatusCode::METHOD_NOT_ALLOWED,
            "Dashboard is read-only",
            Some("GET, HEAD"),
        );
    }

    let services = match services::load() {
        Ok(data) => data,
        Err(error) => {
            eprintln!("pm-daemon: dashboard services load error: {error}");
            return plain_response(
                method,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not load the PM service registry",
                None,
            );
        }
    };
    let ports = match load_ports_read_only() {
        Ok(data) => data,
        Err(error) => {
            eprintln!("pm-daemon: dashboard ports load error: {error}");
            PortsData::default()
        }
    };

    let foreground = match foreground_processes::load() {
        Ok(data) => data,
        Err(error) => {
            eprintln!("pm-daemon: dashboard foreground process load error: {error}");
            ForegroundProcessesData::default()
        }
    };
    let tailnet_links = tokio::task::spawn_blocking(discover_tailnet_links)
        .await
        .unwrap_or_default();
    let snapshot = build_snapshot_with_extras(
        DashboardSources {
            routes,
            services: &services,
            ports: &ports,
            foreground: &foreground,
            tailnet_links: &tailnet_links,
        },
        proxy_port,
        services::pid_alive,
        foreground_processes::is_running,
    );
    dashboard_snapshot_response(method, &snapshot, proxy_port)
}

#[cfg(test)]
fn dashboard_response<F>(
    method: &Method,
    routes: &RoutesData,
    services: &ServicesData,
    proxy_port: u16,
    is_alive: F,
) -> Response<Full<Bytes>>
where
    F: FnMut(u32) -> bool,
{
    let snapshot = build_snapshot(routes, services, proxy_port, is_alive);
    dashboard_snapshot_response(method, &snapshot, proxy_port)
}

fn dashboard_snapshot_response(
    method: &Method,
    snapshot: &DashboardSnapshot,
    proxy_port: u16,
) -> Response<Full<Bytes>> {
    let html = render_html(snapshot, proxy_port);
    let content_length = html.len();
    let body = if method == Method::HEAD {
        Bytes::new()
    } else {
        Bytes::from(html)
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .header(CACHE_CONTROL, "no-store")
        .header(CONTENT_LENGTH, content_length.to_string())
        .header("content-security-policy", CONTENT_SECURITY_POLICY)
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer")
        .body(Full::new(body))
        .expect("valid dashboard response")
}

fn plain_response(
    method: &Method,
    status: StatusCode,
    message: &str,
    allow: Option<&str>,
) -> Response<Full<Bytes>> {
    let body_text = format!("{message}\n");
    let content_length = body_text.len();
    let body = if method == Method::HEAD {
        Bytes::new()
    } else {
        Bytes::from(body_text)
    };
    let mut builder = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(CACHE_CONTROL, "no-store")
        .header(CONTENT_LENGTH, content_length.to_string())
        .header("x-content-type-options", "nosniff");
    if let Some(value) = allow {
        builder = builder.header(ALLOW, value);
    }
    builder
        .body(Full::new(body))
        .expect("valid dashboard error response")
}

#[cfg(test)]
fn build_snapshot<F>(
    routes: &RoutesData,
    services: &ServicesData,
    proxy_port: u16,
    is_alive: F,
) -> DashboardSnapshot
where
    F: FnMut(u32) -> bool,
{
    let ports = PortsData::default();
    let foreground = ForegroundProcessesData::default();
    let tailnet_links = HashMap::new();
    build_snapshot_with_extras(
        DashboardSources {
            routes,
            services,
            ports: &ports,
            foreground: &foreground,
            tailnet_links: &tailnet_links,
        },
        proxy_port,
        is_alive,
        |_| false,
    )
}

fn build_snapshot_with_extras<F, G>(
    sources: DashboardSources<'_>,
    proxy_port: u16,
    mut is_alive: F,
    mut is_foreground_alive: G,
) -> DashboardSnapshot
where
    F: FnMut(u32) -> bool,
    G: FnMut(&foreground_processes::ForegroundProcessState) -> bool,
{
    let DashboardSources {
        routes,
        services,
        ports,
        foreground,
        tailnet_links,
    } = sources;
    let mut apps = Vec::new();

    for (project_key, project_services) in &services.projects {
        let (workspace, project) = project_key
            .split_once('/')
            .map(|(workspace, project)| (workspace.to_string(), project.to_string()))
            .unwrap_or_else(|| ("unknown".to_string(), project_key.clone()));

        for (service, state) in project_services {
            let expected_hostnames = expected_route_hostnames(&workspace, &project, service);
            let preferred = preferred_route(
                &routes.entries,
                project_key,
                service,
                state.port,
                &expected_hostnames,
            );
            let has_route_evidence = routes.entries.iter().any(|route| {
                (route.project_key == *project_key && route.service_key == *service)
                    || expected_hostnames
                        .iter()
                        .any(|hostname| route.hostname.eq_ignore_ascii_case(hostname))
            });
            let alive = is_alive(state.pid);
            let status = if !alive {
                AppStatus::Stale
            } else if preferred.is_some() {
                AppStatus::Running
            } else if has_route_evidence {
                AppStatus::RouteMismatch
            } else {
                AppStatus::RouteMissing
            };
            let url = if status.has_healthy_route() {
                preferred.map(|route| public_url(&route.hostname, proxy_port))
            } else {
                None
            };

            apps.push(DashboardApp {
                workspace: workspace.clone(),
                project: project.clone(),
                service: service.clone(),
                kind: service_kind(ports, project_key, service, state.port),
                mode: ProcessMode::Background,
                pid: state.pid,
                upstream_port: Some(state.port),
                started_at: state.started_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                url,
                tailnet_urls: if alive {
                    tailnet_links.get(&state.port).cloned().unwrap_or_default()
                } else {
                    Vec::new()
                },
                status,
            });
        }
    }

    for process in foreground.processes.values() {
        let alive = is_foreground_alive(process);
        apps.push(DashboardApp {
            workspace: process.workspace.clone(),
            project: process.project.clone(),
            service: process.program.clone(),
            kind: None,
            mode: ProcessMode::Foreground,
            pid: process.pid,
            upstream_port: None,
            started_at: process
                .started_at
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string(),
            url: None,
            tailnet_urls: Vec::new(),
            status: if alive {
                AppStatus::Running
            } else {
                AppStatus::Stale
            },
        });
    }

    apps.sort_by(|left, right| {
        (
            &left.workspace,
            &left.project,
            left.mode.label(),
            &left.service,
            left.pid,
        )
            .cmp(&(
                &right.workspace,
                &right.project,
                right.mode.label(),
                &right.service,
                right.pid,
            ))
    });
    let background_count = apps
        .iter()
        .filter(|app| app.mode == ProcessMode::Background && app.status.is_process_running())
        .count();
    let foreground_count = apps
        .iter()
        .filter(|app| app.mode == ProcessMode::Foreground && app.status.is_process_running())
        .count();
    let attention_count = apps
        .iter()
        .filter(|app| app.status != AppStatus::Running)
        .count();

    DashboardSnapshot {
        apps,
        background_count,
        foreground_count,
        attention_count,
    }
}

fn load_ports_read_only() -> anyhow::Result<PortsData> {
    let path = crate::config::ports_path();
    let content =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

fn service_kind(
    ports: &PortsData,
    project_key: &str,
    service_key: &str,
    runtime_port: u16,
) -> Option<PortKind> {
    ports
        .projects
        .get(project_key)?
        .services
        .get(service_key)
        .filter(|allocation| allocation.port == runtime_port)
        .map(|allocation| allocation.kind)
}

fn preferred_route<'a>(
    routes: &'a [RouteEntry],
    project_key: &str,
    service: &str,
    upstream_port: u16,
    expected_hostnames: &[String],
) -> Option<&'a RouteEntry> {
    expected_hostnames.iter().find_map(|hostname| {
        // Match the reverse proxy's first-entry-wins lookup. A later duplicate
        // must not make the dashboard claim that a broken effective route works.
        routes
            .iter()
            .find(|route| route.hostname.eq_ignore_ascii_case(hostname))
            .filter(|route| {
                route.project_key == project_key
                    && route.service_key == service
                    && route.upstream_port == upstream_port
            })
    })
}

fn expected_route_hostnames(workspace: &str, project: &str, service: &str) -> Vec<String> {
    let short = format!("{service}.{project}.localhost");
    let canonical = format!("{service}.{project}.{workspace}.localhost");
    let mut hostnames = Vec::with_capacity(2);

    if workspace == "default" {
        hostnames.push(short);
    }
    hostnames.push(canonical);
    hostnames
        .iter()
        .filter(|hostname| {
            is_safe_route_hostname(hostname) && !is_dashboard_host(hostname.as_str(), &[])
        })
        .cloned()
        .collect()
}

fn is_safe_route_hostname(hostname: &str) -> bool {
    hostname.len() <= 253
        && hostname.to_ascii_lowercase().ends_with(".localhost")
        && hostname.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
}

fn public_url(hostname: &str, proxy_port: u16) -> String {
    if proxy_port == 80 {
        format!("http://{hostname}/")
    } else {
        format!("http://{hostname}:{proxy_port}/")
    }
}

#[derive(Debug, Default, Deserialize)]
struct TailscaleServeStatus {
    #[serde(default, rename = "Web")]
    web: HashMap<String, TailscaleWebServer>,
}

#[derive(Debug, Default, Deserialize)]
struct TailscaleWebServer {
    #[serde(default, rename = "Handlers")]
    handlers: HashMap<String, TailscaleHandler>,
}

#[derive(Debug, Default, Deserialize)]
struct TailscaleHandler {
    #[serde(default, rename = "Proxy")]
    proxy: String,
}

fn discover_tailnet_links() -> HashMap<u16, Vec<String>> {
    static CACHE: OnceLock<Mutex<TailnetCache>> = OnceLock::new();

    let cache = CACHE.get_or_init(|| Mutex::new(TailnetCache::default()));
    let mut cache = cache.lock().unwrap_or_else(|error| error.into_inner());
    if cache
        .refreshed_at
        .is_some_and(|refreshed_at| refreshed_at.elapsed() < TAILSCALE_CACHE_TTL)
    {
        return cache.links.clone();
    }

    cache.links = query_tailnet_links();
    cache.refreshed_at = Some(Instant::now());
    cache.links.clone()
}

#[derive(Debug, Default)]
struct TailnetCache {
    refreshed_at: Option<Instant>,
    links: HashMap<u16, Vec<String>>,
}

fn query_tailnet_links() -> HashMap<u16, Vec<String>> {
    const CANDIDATES: &[&str] = &[
        "tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ];

    let status = CANDIDATES
        .iter()
        .find_map(|program| tailscale_status(program));

    status
        .as_ref()
        .map(tailnet_links_from_status)
        .unwrap_or_default()
}

fn tailscale_status(program: &str) -> Option<TailscaleServeStatus> {
    let mut child = Command::new(program)
        .args(["serve", "status", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Drain concurrently: some Tailscale CLI builds do not exit promptly while
    // their piped status output is waiting for the parent to read it.
    let output = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });

    let started_at = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started_at.elapsed() < TAILSCALE_COMMAND_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = output.join();
                return None;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = output.join();
                return None;
            }
        }
    };
    if !status.success() {
        let _ = output.join();
        return None;
    }

    let stdout = output.join().ok()?.ok()?;
    serde_json::from_slice(&stdout).ok()
}

fn tailnet_links_from_status(status: &TailscaleServeStatus) -> HashMap<u16, Vec<String>> {
    let mut links: HashMap<u16, Vec<String>> = HashMap::new();

    for (authority, server) in &status.web {
        for (path, handler) in &server.handlers {
            let Some(port) = loopback_proxy_port(&handler.proxy) else {
                continue;
            };
            let authority = authority.strip_suffix(":443").unwrap_or(authority.as_str());
            let path = if path.starts_with('/') {
                path.to_string()
            } else {
                format!("/{path}")
            };
            links
                .entry(port)
                .or_default()
                .push(format!("https://{authority}{path}"));
        }
    }

    for urls in links.values_mut() {
        urls.sort();
        urls.dedup();
    }
    links
}

fn loopback_proxy_port(proxy: &str) -> Option<u16> {
    let remainder = proxy
        .strip_prefix("http://")
        .or_else(|| proxy.strip_prefix("https://"))?;
    let authority = remainder.split('/').next()?;

    if let Some(port) = authority.strip_prefix("[::1]:") {
        return port.parse().ok();
    }

    let (host, port) = authority.rsplit_once(':')?;
    if host != "127.0.0.1" && !host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    port.parse().ok()
}

fn render_html(snapshot: &DashboardSnapshot, proxy_port: u16) -> String {
    let _ = proxy_port;
    let mut frontend_cards = String::new();
    let mut supporting_rows = String::new();
    let mut frontend_count = 0;
    let mut supporting_count = 0;

    for app in &snapshot.apps {
        if app.kind == Some(PortKind::Frontend) {
            frontend_count += 1;
            render_frontend_card(&mut frontend_cards, app);
        } else {
            supporting_count += 1;
            render_supporting_process(&mut supporting_rows, app);
        }
    }

    let frontend_content = if frontend_count > 0 {
        format!(r#"<div class="grid">{frontend_cards}</div>"#)
    } else if snapshot.apps.is_empty() {
        r#"<section class="empty">
  <div class="empty-icon">0</div>
  <h2>No PM processes are running</h2>
  <p>Start a configured service or <code>pm run -- &lt;command&gt;</code> and it will appear here.</p>
</section>"#
            .to_string()
    } else {
        r#"<section class="empty compact">
  <div class="empty-icon">0</div>
  <h2>No frontend applications are running</h2>
  <p>Backend and other PM processes are listed below.</p>
</section>"#
            .to_string()
    };

    let supporting_section = if supporting_count == 0 {
        String::new()
    } else {
        format!(
            r#"<section class="supporting" aria-labelledby="supporting-title">
      <div class="section-heading supporting-heading">
        <div>
          <p class="section-kicker">Non-navigable</p>
          <h2 id="supporting-title">Backend &amp; other processes</h2>
        </div>
        <span>{supporting_count} processes</span>
      </div>
      <ul class="process-list">{supporting_rows}</ul>
    </section>"#
        )
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="5">
  <title>PM Processes</title>
  <style>
    :root {{ color-scheme: dark; font-family: ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #09090b; color: #f4f4f5; }}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; min-height: 100vh; background: radial-gradient(circle at top left, #172554 0, transparent 34rem), #09090b; }}
    main {{ width: min(1120px, calc(100% - 32px)); margin: 0 auto; padding: 64px 0 80px; }}
    header {{ display: flex; align-items: flex-end; justify-content: space-between; gap: 24px; margin-bottom: 28px; }}
    .eyebrow {{ margin: 0 0 8px; color: #93c5fd; font: 700 12px/1.3 ui-monospace, SFMono-Regular, Menlo, monospace; letter-spacing: .12em; text-transform: uppercase; }}
    h1 {{ margin: 0; font-size: clamp(34px, 7vw, 64px); line-height: .95; letter-spacing: -.055em; }}
    .summary {{ display: flex; gap: 10px; flex-wrap: wrap; justify-content: flex-end; }}
    .summary span {{ border: 1px solid #27272a; border-radius: 999px; padding: 8px 12px; color: #a1a1aa; background: #18181bcc; font-size: 13px; }}
    .summary strong {{ color: #fafafa; }}
    .section-heading {{ display: flex; align-items: flex-end; justify-content: space-between; gap: 20px; margin: 0 0 14px; }}
    .section-heading h2 {{ margin: 0; font-size: 19px; letter-spacing: -.02em; }}
    .section-heading > span, .section-heading p {{ margin: 0; color: #71717a; font-size: 12px; }}
    .section-kicker {{ margin: 0 0 4px !important; color: #60a5fa !important; font: 700 10px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace !important; letter-spacing: .1em; text-transform: uppercase; }}
    .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(min(300px, 100%), 1fr)); gap: 14px; }}
    .card {{ display: block; min-width: 0; border: 1px solid #27272a; border-radius: 18px; padding: 20px; color: inherit; background: #121216e6; box-shadow: 0 18px 50px #0005; text-decoration: none; }}
    .card-link {{ cursor: pointer; transition: border-color .16s ease, box-shadow .16s ease, transform .16s ease; }}
    .card-link:hover {{ border-color: #3b82f6; box-shadow: 0 20px 54px #0007, 0 0 0 1px #3b82f633; transform: translateY(-2px); }}
    .card-link:focus-visible {{ outline: 3px solid #60a5fa; outline-offset: 3px; border-color: #60a5fa; }}
    .card-unavailable {{ border-style: dashed; }}
    .card-head {{ display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }}
    .identity {{ min-width: 0; }}
    .project {{ overflow: hidden; color: #a1a1aa; font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; text-overflow: ellipsis; white-space: nowrap; }}
    h3 {{ margin: 5px 0 0; font-size: 23px; letter-spacing: -.025em; }}
    .badges {{ display: flex; align-items: flex-end; gap: 6px; flex-direction: column; }}
    .status {{ flex: 0 0 auto; border-radius: 999px; padding: 6px 9px; font-size: 11px; font-weight: 750; }}
    .status.running {{ color: #86efac; background: #14532d66; }}
    .status.warning {{ color: #fde68a; background: #78350f66; }}
    .status.stale {{ color: #d4d4d8; background: #3f3f4666; }}
    .mode {{ border: 1px solid #3f3f46; border-radius: 999px; padding: 4px 8px; color: #a1a1aa; font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; }}
    .mode.foreground {{ border-color: #1d4ed8; color: #bfdbfe; }}
    .links {{ display: grid; gap: 8px; margin: 20px 0; }}
    .url {{ display: flex; min-width: 0; align-items: center; gap: 8px; overflow: hidden; color: #93c5fd; font: 13px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; white-space: nowrap; text-decoration: none; }}
    .link-kind {{ flex: 0 0 auto; border: 1px solid #3f3f46; border-radius: 5px; padding: 1px 5px; color: #a1a1aa; font: 700 9px/1.5 ui-sans-serif, sans-serif; text-transform: uppercase; letter-spacing: .05em; }}
    .link-value {{ overflow: hidden; text-overflow: ellipsis; }}
    .card-link:hover .url {{ color: #dbeafe; }}
    .url.unavailable {{ color: #71717a; }}
    dl {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin: 0; }}
    dt {{ color: #71717a; font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }}
    dd {{ overflow: hidden; margin: 4px 0 0; color: #d4d4d8; font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; text-overflow: ellipsis; white-space: nowrap; }}
    .card-action {{ display: flex; align-items: center; justify-content: space-between; margin-top: 18px; padding-top: 15px; border-top: 1px solid #27272a; color: #bfdbfe; font-size: 12px; font-weight: 700; }}
    .card-action svg {{ width: 15px; height: 15px; }}
    .card-link:hover .card-action {{ color: #fff; }}
    .supporting {{ margin-top: 40px; }}
    .supporting-heading {{ padding: 0 2px; }}
    .process-list {{ display: grid; margin: 0; padding: 0; border-top: 1px solid #27272a; list-style: none; }}
    .process-row {{ display: grid; min-width: 0; grid-template-columns: minmax(180px, 1fr) auto minmax(300px, 1.25fr); align-items: center; gap: 20px; padding: 16px 2px; border-bottom: 1px solid #27272a; }}
    .process-row h3 {{ font-size: 16px; }}
    .process-badges {{ display: flex; align-items: center; justify-content: flex-end; gap: 6px; flex-wrap: wrap; }}
    .kind {{ border: 1px solid #3f3f46; border-radius: 999px; padding: 4px 8px; color: #a1a1aa; font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; }}
    .process-row dl {{ grid-template-columns: repeat(3, minmax(0, 1fr)); }}
    .empty {{ border: 1px dashed #3f3f46; border-radius: 20px; padding: 64px 24px; color: #a1a1aa; text-align: center; background: #12121699; }}
    .empty.compact {{ padding: 42px 24px; }}
    .empty-icon {{ display: grid; width: 48px; height: 48px; margin: 0 auto 18px; place-items: center; border: 1px solid #3f3f46; border-radius: 14px; color: #71717a; font: 700 16px/1 ui-monospace, SFMono-Regular, Menlo, monospace; }}
    .empty h2 {{ color: #f4f4f5; }}
    .empty p {{ margin-bottom: 0; }}
    code {{ border: 1px solid #3f3f46; border-radius: 6px; padding: 2px 6px; color: #dbeafe; background: #18181b; }}
    footer {{ display: flex; justify-content: space-between; gap: 20px; margin-top: 24px; color: #71717a; font-size: 12px; }}
    footer a {{ color: #a1a1aa; }}
    @media (prefers-reduced-motion: reduce) {{ .card-link {{ transition: none; }} .card-link:hover {{ transform: none; }} }}
    @media (max-width: 760px) {{ main {{ padding-top: 36px; }} header, .section-heading {{ align-items: flex-start; flex-direction: column; }} .summary {{ justify-content: flex-start; }} dl, .process-row dl {{ grid-template-columns: 1fr 1fr; }} .process-row {{ grid-template-columns: 1fr; gap: 12px; }} .process-row dl > div:last-child {{ grid-column: 1 / -1; }} .process-badges {{ justify-content: flex-start; }} }}
  </style>
</head>
<body>
  <main>
    <header>
      <div>
        <p class="eyebrow">Local orchestrator</p>
        <h1>PM Processes</h1>
      </div>
      <div class="summary">
        <span><strong>{background}</strong> background</span>
        <span><strong>{foreground}</strong> foreground</span>
        <span><strong>{attention}</strong> need attention</span>
      </div>
    </header>
    <section class="frontend-section" aria-labelledby="frontends-title">
      <div class="section-heading">
        <div>
          <p class="section-kicker">Destinations</p>
          <h2 id="frontends-title">Frontend applications</h2>
        </div>
        <span>Click a card to open the app</span>
      </div>
      {frontend_content}
    </section>
    {supporting_section}
    <footer>
      <span>Read-only · refreshes every 5 seconds</span>
      <a href="/">Refresh now</a>
    </footer>
  </main>
</body>
</html>"#,
        background = snapshot.background_count,
        foreground = snapshot.foreground_count,
        attention = snapshot.attention_count,
    )
}

fn preferred_navigation(app: &DashboardApp) -> Option<(&'static str, &str)> {
    app.tailnet_urls
        .first()
        .map(|url| ("Tailnet", url.as_str()))
        .or_else(|| app.url.as_deref().map(|url| ("Local", url)))
}

fn render_frontend_card(output: &mut String, app: &DashboardApp) {
    let workspace = escape_html(&app.workspace);
    let project = escape_html(&app.project);
    let service = escape_html(&app.service);
    let status = app.status;
    let mode = app.mode;
    let navigation = preferred_navigation(app);

    let closing_tag = if let Some((destination_kind, destination)) = navigation {
        let destination = escape_html(destination);
        let aria_label = escape_html(&format!(
            "Open {} / {} {} via {}",
            app.workspace, app.project, app.service, destination_kind
        ));
        let _ = write!(
            output,
            r#"<a class="card card-link" href="{destination}" aria-label="{aria_label}">"#
        );
        "</a>"
    } else {
        output.push_str(r#"<article class="card card-unavailable">"#);
        "</article>"
    };

    let _ = write!(
        output,
        r#"<div class="card-head">
    <div class="identity">
      <div class="project">{workspace} / {service}</div>
      <h3>{project}</h3>
    </div>
    <div class="badges">
      <span class="status {status_class}">{status_label}</span>
      <span class="mode {mode_class}">{mode_label}</span>
    </div>
  </div>"#,
        status_class = status.css_class(),
        status_label = status.label(),
        mode_class = mode.css_class(),
        mode_label = mode.label(),
    );

    render_addresses(output, app);

    let port = app
        .upstream_port
        .map(|port| port.to_string())
        .unwrap_or_else(|| "—".to_string());
    let _ = write!(
        output,
        r#"<dl>
    <div><dt>Port</dt><dd>{port}</dd></div>
    <div><dt>PID</dt><dd>{pid}</dd></div>
    <div><dt>Started</dt><dd>{started}</dd></div>
  </dl>"#,
        port = port,
        pid = app.pid,
        started = escape_html(&app.started_at),
    );

    if let Some((destination_kind, _)) = navigation {
        let _ = write!(
            output,
            r#"<span class="card-action">Open via {destination_kind}<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3h8v8M13 3 3 13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg></span>"#
        );
    }
    output.push_str(closing_tag);
}

fn render_supporting_process(output: &mut String, app: &DashboardApp) {
    let workspace = escape_html(&app.workspace);
    let project = escape_html(&app.project);
    let service = escape_html(&app.service);
    let status = app.status;
    let mode = app.mode;
    let kind = app.kind.map(PortKind::as_str).unwrap_or("process");

    let _ = write!(
        output,
        r#"<li class="process-row">
  <div class="identity">
    <div class="project">{workspace} / {service}</div>
    <h3>{project}</h3>
  </div>
  <div class="process-badges">
    <span class="status {status_class}">{status_label}</span>
    <span class="kind">{kind}</span>
    <span class="mode {mode_class}">{mode_label}</span>
  </div>"#,
        status_class = status.css_class(),
        status_label = status.label(),
        mode_class = mode.css_class(),
        mode_label = mode.label(),
    );

    let port = app
        .upstream_port
        .map(|port| port.to_string())
        .unwrap_or_else(|| "—".to_string());
    let _ = write!(
        output,
        r#"<dl>
    <div><dt>Port</dt><dd>{port}</dd></div>
    <div><dt>PID</dt><dd>{pid}</dd></div>
    <div><dt>Started</dt><dd>{started}</dd></div>
  </dl>
</li>"#,
        port = port,
        pid = app.pid,
        started = escape_html(&app.started_at),
    );
}

fn render_addresses(output: &mut String, app: &DashboardApp) {
    output.push_str(r#"<div class="links">"#);
    if let Some(url) = &app.url {
        let url = escape_html(url);
        let _ = write!(
            output,
            r#"<span class="url"><span class="link-kind">Local</span><span class="link-value">{url}</span></span>"#
        );
    }
    for tailnet_url in &app.tailnet_urls {
        let tailnet_url = escape_html(tailnet_url);
        let _ = write!(
            output,
            r#"<span class="url"><span class="link-kind">Tailnet</span><span class="link-value">{tailnet_url}</span></span>"#
        );
    }
    if app.url.is_none() && app.tailnet_urls.is_empty() {
        let message = if app.mode == ProcessMode::Foreground {
            "No managed HTTP route"
        } else {
            "Route unavailable"
        };
        let _ = write!(output, r#"<span class="url unavailable">{message}</span>"#);
    }
    output.push_str("</div>");
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foreground_processes::ForegroundProcessState;
    use crate::models::{PortProject, PortService};
    use crate::routes::ROUTES_SCHEMA_VERSION;
    use crate::services::ServiceState;
    use chrono::{TimeZone, Utc};
    use http_body_util::BodyExt;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn state(pid: u32, port: u16) -> ServiceState {
        ServiceState {
            pid,
            port,
            started_at: Utc.with_ymd_and_hms(2026, 7, 25, 12, 30, 0).unwrap(),
            log_path: PathBuf::from("/tmp/service.log"),
            dev_cmd: "cargo run".into(),
        }
    }

    fn route(hostname: &str, port: u16, project_key: &str, service_key: &str) -> RouteEntry {
        RouteEntry {
            hostname: hostname.into(),
            upstream_port: port,
            project_key: project_key.into(),
            service_key: service_key.into(),
        }
    }

    fn services(entries: &[(&str, &str, ServiceState)]) -> ServicesData {
        let mut projects: HashMap<String, HashMap<String, ServiceState>> = HashMap::new();
        for (project, service, state) in entries {
            projects
                .entry((*project).into())
                .or_default()
                .insert((*service).into(), state.clone());
        }
        ServicesData { projects }
    }

    fn ports(entries: &[(&str, &str, PortKind, u16)]) -> PortsData {
        let mut data = PortsData::default();
        for (project_key, service_key, kind, port) in entries {
            let (workspace, project) = project_key
                .split_once('/')
                .expect("test project key must include a workspace");
            data.projects
                .entry((*project_key).into())
                .or_insert_with(|| PortProject {
                    workspace: workspace.into(),
                    project: project.into(),
                    path: format!("/tmp/{project}"),
                    services: HashMap::new(),
                })
                .services
                .insert(
                    (*service_key).into(),
                    PortService {
                        kind: *kind,
                        env: kind.env_key().into(),
                        port: *port,
                        locked: false,
                    },
                );
        }
        data
    }

    fn dashboard_app(
        project: &str,
        service: &str,
        kind: Option<PortKind>,
        mode: ProcessMode,
        url: Option<&str>,
        tailnet_urls: &[&str],
    ) -> DashboardApp {
        DashboardApp {
            workspace: "personal".into(),
            project: project.into(),
            service: service.into(),
            kind,
            mode,
            pid: 42,
            upstream_port: (mode == ProcessMode::Background).then_some(18_462),
            started_at: "2026-07-25 12:30:00 UTC".into(),
            url: url.map(str::to_string),
            tailnet_urls: tailnet_urls.iter().map(|url| (*url).to_string()).collect(),
            status: if url.is_some() || !tailnet_urls.is_empty() {
                AppStatus::Running
            } else {
                AppStatus::RouteMissing
            },
        }
    }

    #[test]
    fn dashboard_host_matching_is_exact() {
        let configured = vec!["m5-air.tail4beb79.ts.net".to_string()];
        assert!(is_dashboard_host("pm.localhost", &configured));
        assert!(is_dashboard_host("PM.LOCALHOST", &configured));
        assert!(is_dashboard_host("localhost", &configured));
        assert!(is_dashboard_host("127.0.0.1", &configured));
        assert!(is_dashboard_host("M5-AIR.TAIL4BEB79.TS.NET", &configured));
        assert!(!is_dashboard_host("front.pm.localhost", &configured));
        assert!(!is_dashboard_host("pm.localhost.evil", &configured));
        assert!(!is_dashboard_host("other.tail4beb79.ts.net", &configured));
    }

    #[test]
    fn snapshot_deduplicates_routes_and_prefers_default_short_alias() {
        let services = services(&[("default/blog", "front", state(42, 13042))]);
        let routes = RoutesData {
            version: ROUTES_SCHEMA_VERSION,
            entries: vec![
                route(
                    "front.blog.default.localhost",
                    13042,
                    "default/blog",
                    "front",
                ),
                route("front.blog.localhost", 13042, "default/blog", "front"),
            ],
        };

        let snapshot = build_snapshot(&routes, &services, 7100, |_| true);

        assert_eq!(snapshot.apps.len(), 1);
        assert_eq!(snapshot.background_count, 1);
        assert_eq!(
            snapshot.apps[0].url.as_deref(),
            Some("http://front.blog.localhost:7100/")
        );
    }

    #[test]
    fn snapshot_reports_missing_mismatched_and_stale_routes() {
        let services = services(&[
            ("work/api", "missing", state(1, 11001)),
            ("work/api", "mismatch", state(2, 11002)),
            ("work/api", "stale", state(3, 11003)),
        ]);
        let routes = RoutesData {
            version: ROUTES_SCHEMA_VERSION,
            entries: vec![
                route("mismatch.api.work.localhost", 11999, "work/api", "mismatch"),
                route("stale.api.work.localhost", 11003, "work/api", "stale"),
            ],
        };

        let snapshot = build_snapshot(&routes, &services, 7100, |pid| pid != 3);

        assert_eq!(snapshot.background_count, 2);
        assert_eq!(snapshot.attention_count, 3);
        assert_eq!(snapshot.apps[0].status, AppStatus::RouteMismatch);
        assert_eq!(snapshot.apps[1].status, AppStatus::RouteMissing);
        assert_eq!(snapshot.apps[2].status, AppStatus::Stale);
        assert!(snapshot.apps.iter().all(|app| app.url.is_none()));
    }

    #[test]
    fn public_url_omits_default_http_port() {
        assert_eq!(
            public_url("front.api.work.localhost", 80),
            "http://front.api.work.localhost/"
        );
        assert_eq!(
            public_url("front.api.work.localhost", 7100),
            "http://front.api.work.localhost:7100/"
        );
    }

    #[test]
    fn unsafe_or_reserved_route_is_never_linked() {
        let services = services(&[
            ("work/api", "unsafe", state(1, 11001)),
            ("work/api", "reserved", state(2, 11002)),
            ("work/api", "external", state(3, 11003)),
        ]);
        let routes = RoutesData {
            version: ROUTES_SCHEMA_VERSION,
            entries: vec![
                route(
                    r#"unsafe.api.work.localhost/" onclick="alert(1)"#,
                    11001,
                    "work/api",
                    "unsafe",
                ),
                route("pm.localhost", 11002, "work/api", "reserved"),
                route("evil.example", 11003, "work/api", "external"),
            ],
        };

        let snapshot = build_snapshot(&routes, &services, 7100, |_| true);

        assert_eq!(snapshot.background_count, 3);
        assert!(snapshot.apps.iter().all(|app| app.url.is_none()));
        assert!(
            snapshot
                .apps
                .iter()
                .all(|app| app.status == AppStatus::RouteMismatch)
        );
    }

    #[test]
    fn duplicate_hostname_uses_the_same_first_entry_as_the_proxy() {
        let services = services(&[("work/api", "back", state(42, 11001))]);
        let routes = RoutesData {
            version: ROUTES_SCHEMA_VERSION,
            entries: vec![
                route("back.api.work.localhost", 11999, "work/api", "back"),
                route("back.api.work.localhost", 11001, "work/api", "back"),
            ],
        };

        let snapshot = build_snapshot(&routes, &services, 7100, |_| true);

        assert_eq!(snapshot.background_count, 1);
        assert_eq!(snapshot.attention_count, 1);
        assert_eq!(snapshot.apps[0].status, AppStatus::RouteMismatch);
        assert!(snapshot.apps[0].url.is_none());
    }

    #[test]
    fn rendering_escapes_registry_strings() {
        let snapshot = DashboardSnapshot {
            apps: vec![DashboardApp {
                workspace: r#"<script>alert("workspace")</script>"#.into(),
                project: "api&admin".into(),
                service: r#"front"><img src=x>"#.into(),
                kind: Some(PortKind::Frontend),
                mode: ProcessMode::Background,
                pid: 1,
                upstream_port: Some(3000),
                started_at: "2026-07-25 12:30:00 UTC".into(),
                url: Some(r#"http://front.localhost/?q="><script>"#.into()),
                tailnet_urls: Vec::new(),
                status: AppStatus::Running,
            }],
            background_count: 1,
            foreground_count: 0,
            attention_count: 0,
        };

        let html = render_html(&snapshot, 7100);

        assert!(!html.contains("<script>alert"));
        assert!(!html.contains(r#"<img src=x>"#));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("api&amp;admin"));
        assert!(html.contains("&quot;&gt;&lt;img"));
        assert!(html.contains("&quot;&gt;&lt;script&gt;"));
    }

    #[test]
    fn snapshot_shows_background_foreground_and_tailnet_links() {
        let services = services(&[("personal/web", "front", state(42, 18_462))]);
        let routes = RoutesData {
            version: ROUTES_SCHEMA_VERSION,
            entries: vec![route(
                "front.web.personal.localhost",
                18_462,
                "personal/web",
                "front",
            )],
        };
        let mut foreground = ForegroundProcessesData::default();
        foreground.processes.insert(
            "77".into(),
            ForegroundProcessState {
                workspace: "personal".into(),
                project: "worker".into(),
                pid: 77,
                started_at: Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap(),
                program: "pnpm".into(),
                start_token: None,
            },
        );
        let tailnet_links =
            HashMap::from([(18_462, vec!["https://m5-air.example.ts.net:8443/".into()])]);
        let ports = ports(&[("personal/web", "front", PortKind::Frontend, 18_462)]);

        let snapshot = build_snapshot_with_extras(
            DashboardSources {
                routes: &routes,
                services: &services,
                ports: &ports,
                foreground: &foreground,
                tailnet_links: &tailnet_links,
            },
            7100,
            |_| true,
            |_| true,
        );

        assert_eq!(snapshot.background_count + snapshot.foreground_count, 2);
        assert_eq!(snapshot.background_count, 1);
        assert_eq!(snapshot.foreground_count, 1);
        assert_eq!(snapshot.apps[0].mode, ProcessMode::Background);
        assert_eq!(snapshot.apps[0].kind, Some(PortKind::Frontend));
        assert_eq!(
            snapshot.apps[0].tailnet_urls,
            vec!["https://m5-air.example.ts.net:8443/"]
        );
        assert_eq!(snapshot.apps[1].mode, ProcessMode::Foreground);
        assert_eq!(snapshot.apps[1].kind, None);
        assert_eq!(snapshot.apps[1].upstream_port, None);
    }

    #[test]
    fn snapshot_uses_registered_port_kind_and_runtime_port() {
        let services = services(&[
            ("personal/site", "web", state(1, 18_001)),
            ("personal/site", "front", state(2, 22_001)),
            ("personal/site", "client", state(3, 18_002)),
        ]);
        let ports = ports(&[
            ("personal/site", "web", PortKind::Frontend, 18_001),
            ("personal/site", "front", PortKind::Backend, 22_001),
            ("personal/site", "client", PortKind::Frontend, 18_999),
        ]);

        let snapshot = build_snapshot_with_extras(
            DashboardSources {
                routes: &RoutesData::default(),
                services: &services,
                ports: &ports,
                foreground: &ForegroundProcessesData::default(),
                tailnet_links: &HashMap::new(),
            },
            7100,
            |_| true,
            |_| false,
        );

        let kind_for = |service: &str| {
            snapshot
                .apps
                .iter()
                .find(|app| app.service == service)
                .and_then(|app| app.kind)
        };
        assert_eq!(kind_for("web"), Some(PortKind::Frontend));
        assert_eq!(kind_for("front"), Some(PortKind::Backend));
        assert_eq!(kind_for("client"), None);
    }

    #[test]
    fn rendering_makes_only_frontends_clickable_cards() {
        let snapshot = DashboardSnapshot {
            apps: vec![
                dashboard_app(
                    "web",
                    "front",
                    Some(PortKind::Frontend),
                    ProcessMode::Background,
                    Some("http://front.web.personal.localhost:7100/"),
                    &["https://m5-air.example.ts.net:8443/"],
                ),
                dashboard_app(
                    "web",
                    "back",
                    Some(PortKind::Backend),
                    ProcessMode::Background,
                    Some("http://back.web.personal.localhost:7100/"),
                    &["https://m5-air.example.ts.net:9443/"],
                ),
                dashboard_app("worker", "pnpm", None, ProcessMode::Foreground, None, &[]),
            ],
            background_count: 2,
            foreground_count: 1,
            attention_count: 1,
        };

        let html = render_html(&snapshot, 7100);

        assert_eq!(html.matches(r#"<a class="card card-link""#).count(), 1);
        assert_eq!(html.matches(r#"<li class="process-row">"#).count(), 2);
        assert!(html.contains(r#"href="https://m5-air.example.ts.net:8443/""#));
        assert!(html.contains(r#"aria-label="Open personal / web front via Tailnet""#));
        assert!(!html.contains(r#"href="http://front.web.personal.localhost:7100/""#));
        assert!(!html.contains("http://back.web.personal.localhost:7100/"));
        assert!(!html.contains("https://m5-air.example.ts.net:9443/"));
        assert!(html.contains("Backend &amp; other processes"));
        assert!(html.contains("Open via Tailnet"));
    }

    #[test]
    fn frontend_card_falls_back_to_local_and_disables_missing_routes() {
        let snapshot = DashboardSnapshot {
            apps: vec![
                dashboard_app(
                    "local",
                    "web",
                    Some(PortKind::Frontend),
                    ProcessMode::Background,
                    Some("http://web.local.personal.localhost:7100/"),
                    &[],
                ),
                dashboard_app(
                    "offline",
                    "client",
                    Some(PortKind::Frontend),
                    ProcessMode::Background,
                    None,
                    &[],
                ),
            ],
            background_count: 2,
            foreground_count: 0,
            attention_count: 1,
        };

        let html = render_html(&snapshot, 7100);

        assert_eq!(html.matches(r#"class="card card-link""#).count(), 1);
        assert_eq!(html.matches(r#"class="card card-unavailable""#).count(), 1);
        assert!(html.contains(r#"href="http://web.local.personal.localhost:7100/""#));
        assert!(html.contains("Open via Local"));
        assert!(html.contains("Route unavailable"));
    }

    #[test]
    fn backend_only_snapshot_keeps_process_list_instead_of_global_empty_state() {
        let snapshot = DashboardSnapshot {
            apps: vec![dashboard_app(
                "api",
                "back",
                Some(PortKind::Backend),
                ProcessMode::Background,
                Some("http://back.api.personal.localhost:7100/"),
                &[],
            )],
            background_count: 1,
            foreground_count: 0,
            attention_count: 0,
        };

        let html = render_html(&snapshot, 7100);

        assert!(html.contains("No frontend applications are running"));
        assert!(!html.contains("No PM processes are running"));
        assert_eq!(html.matches(r#"<li class="process-row">"#).count(), 1);
        assert!(!html.contains("http://back.api.personal.localhost:7100/"));
    }

    #[test]
    fn parses_tailscale_serve_links_by_loopback_port() {
        let status: TailscaleServeStatus = serde_json::from_value(serde_json::json!({
            "Web": {
                "m5-air.example.ts.net:443": {
                    "Handlers": {
                        "/": {"Proxy": "http://127.0.0.1:18368"}
                    }
                },
                "m5-air.example.ts.net:8443": {
                    "Handlers": {
                        "/learn": {"Proxy": "http://localhost:18462"},
                        "/ignored": {"Proxy": "http://10.0.0.1:3000"}
                    }
                }
            }
        }))
        .unwrap();

        let links = tailnet_links_from_status(&status);
        assert_eq!(links[&18_368], vec!["https://m5-air.example.ts.net/"]);
        assert_eq!(
            links[&18_462],
            vec!["https://m5-air.example.ts.net:8443/learn"]
        );
        assert!(!links.contains_key(&3000));
        assert_eq!(loopback_proxy_port("http://[::1]:7100"), Some(7100));
    }

    #[tokio::test]
    async fn dashboard_http_contract_supports_get_head_and_rejects_mutation() {
        let routes = RoutesData::default();
        let services = ServicesData::default();

        let get = dashboard_response(&Method::GET, &routes, &services, 7100, |_| true);
        assert_eq!(get.status(), StatusCode::OK);
        assert_eq!(
            get.headers().get(CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(get.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        assert!(get.headers().contains_key("content-security-policy"));
        let get_body = get.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&get_body).contains("No PM processes are running"));

        let head = dashboard_response(&Method::HEAD, &routes, &services, 7100, |_| true);
        assert_eq!(head.status(), StatusCode::OK);
        assert_ne!(head.headers().get(CONTENT_LENGTH).unwrap(), "0");
        assert!(
            head.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );

        let post = response(&Method::POST, "/", &routes, 7100).await;
        assert_eq!(post.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(post.headers().get(ALLOW).unwrap(), "GET, HEAD");

        let missing = response(&Method::GET, "/missing", &routes, 7100).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let missing_head = response(&Method::HEAD, "/missing", &routes, 7100).await;
        assert_eq!(missing_head.status(), StatusCode::NOT_FOUND);
        assert_ne!(missing_head.headers().get(CONTENT_LENGTH).unwrap(), "0");
        assert!(
            missing_head
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );

        let failed_head = plain_response(
            &Method::HEAD,
            StatusCode::INTERNAL_SERVER_ERROR,
            "registry failed",
            None,
        );
        assert_eq!(failed_head.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            failed_head
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );
    }
}
