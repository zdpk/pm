//! Read-only HTML dashboard for services managed by the PM orchestrator.
//!
//! `services.json` is the process registry. `routes.json` is joined only to
//! find a browser URL for each service, so canonical and short aliases never
//! create duplicate dashboard entries.

use crate::foreground_processes::{self, ForegroundProcessesData};
use crate::routes::{RouteEntry, RoutesData};
use crate::services::{self, ServicesData};
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
        routes,
        &services,
        &foreground,
        &tailnet_links,
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
    build_snapshot_with_extras(
        routes,
        services,
        &ForegroundProcessesData::default(),
        &HashMap::new(),
        proxy_port,
        is_alive,
        |_| false,
    )
}

fn build_snapshot_with_extras<F, G>(
    routes: &RoutesData,
    services: &ServicesData,
    foreground: &ForegroundProcessesData,
    tailnet_links: &HashMap<u16, Vec<String>>,
    proxy_port: u16,
    mut is_alive: F,
    mut is_foreground_alive: G,
) -> DashboardSnapshot
where
    F: FnMut(u32) -> bool,
    G: FnMut(&foreground_processes::ForegroundProcessState) -> bool,
{
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
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

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
                return None;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }

    let mut stdout = Vec::new();
    child.stdout.take()?.read_to_end(&mut stdout).ok()?;
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
    let mut cards = String::new();

    if snapshot.apps.is_empty() {
        cards.push_str(
            r#"<section class="empty">
  <div class="empty-icon">0</div>
  <h2>No PM processes are running</h2>
  <p>Start a configured service or <code>pm run -- &lt;command&gt;</code> and it will appear here.</p>
</section>"#,
        );
    } else {
        for app in &snapshot.apps {
            render_app(&mut cards, app);
        }
    }

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
    .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 14px; }}
    .card {{ min-width: 0; border: 1px solid #27272a; border-radius: 18px; padding: 20px; background: #121216e6; box-shadow: 0 18px 50px #0005; }}
    .card-head {{ display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }}
    .identity {{ min-width: 0; }}
    .project {{ overflow: hidden; color: #a1a1aa; font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; text-overflow: ellipsis; white-space: nowrap; }}
    h2 {{ margin: 5px 0 0; font-size: 23px; letter-spacing: -.025em; }}
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
    a.url:hover {{ color: #dbeafe; text-decoration: underline; }}
    .url.unavailable {{ color: #71717a; }}
    dl {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin: 0; }}
    dt {{ color: #71717a; font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }}
    dd {{ overflow: hidden; margin: 4px 0 0; color: #d4d4d8; font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; text-overflow: ellipsis; white-space: nowrap; }}
    .empty {{ border: 1px dashed #3f3f46; border-radius: 20px; padding: 64px 24px; color: #a1a1aa; text-align: center; background: #12121699; }}
    .empty-icon {{ display: grid; width: 48px; height: 48px; margin: 0 auto 18px; place-items: center; border: 1px solid #3f3f46; border-radius: 14px; color: #71717a; font: 700 16px/1 ui-monospace, SFMono-Regular, Menlo, monospace; }}
    .empty h2 {{ color: #f4f4f5; }}
    .empty p {{ margin-bottom: 0; }}
    code {{ border: 1px solid #3f3f46; border-radius: 6px; padding: 2px 6px; color: #dbeafe; background: #18181b; }}
    footer {{ display: flex; justify-content: space-between; gap: 20px; margin-top: 24px; color: #71717a; font-size: 12px; }}
    footer a {{ color: #a1a1aa; }}
    @media (max-width: 640px) {{ main {{ padding-top: 36px; }} header {{ align-items: flex-start; flex-direction: column; }} .summary {{ justify-content: flex-start; }} dl {{ grid-template-columns: 1fr 1fr; }} }}
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
    <section class="grid">{cards}</section>
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

fn render_app(output: &mut String, app: &DashboardApp) {
    let workspace = escape_html(&app.workspace);
    let project = escape_html(&app.project);
    let service = escape_html(&app.service);
    let status = app.status;
    let url = app.url.as_deref().map(escape_html);
    let mode = app.mode;

    let _ = write!(
        output,
        r#"<article class="card">
  <div class="card-head">
    <div class="identity">
      <div class="project">{workspace} / {project}</div>
      <h2>{service}</h2>
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

    output.push_str(r#"<div class="links">"#);
    if let Some(url) = url {
        let _ = write!(
            output,
            r#"<a class="url" href="{url}"><span class="link-kind">Local</span><span class="link-value">{url}</span></a>"#
        );
    }
    for tailnet_url in &app.tailnet_urls {
        let tailnet_url = escape_html(tailnet_url);
        let _ = write!(
            output,
            r#"<a class="url" href="{tailnet_url}"><span class="link-kind">Tailnet</span><span class="link-value">{tailnet_url}</span></a>"#
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
</article>"#,
        port = port,
        pid = app.pid,
        started = escape_html(&app.started_at),
    );
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
                mode: ProcessMode::Background,
                pid: 1,
                upstream_port: Some(3000),
                started_at: "2026-07-25 12:30:00 UTC".into(),
                url: None,
                tailnet_urls: Vec::new(),
                status: AppStatus::RouteMissing,
            }],
            background_count: 0,
            foreground_count: 0,
            attention_count: 1,
        };

        let html = render_html(&snapshot, 7100);

        assert!(!html.contains("<script>alert"));
        assert!(!html.contains(r#"<img src=x>"#));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("api&amp;admin"));
        assert!(html.contains("&quot;&gt;&lt;img"));
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

        let snapshot = build_snapshot_with_extras(
            &routes,
            &services,
            &foreground,
            &tailnet_links,
            7100,
            |_| true,
            |_| true,
        );

        assert_eq!(snapshot.background_count + snapshot.foreground_count, 2);
        assert_eq!(snapshot.background_count, 1);
        assert_eq!(snapshot.foreground_count, 1);
        assert_eq!(snapshot.apps[0].mode, ProcessMode::Background);
        assert_eq!(
            snapshot.apps[0].tailnet_urls,
            vec!["https://m5-air.example.ts.net:8443/"]
        );
        assert_eq!(snapshot.apps[1].mode, ProcessMode::Foreground);
        assert_eq!(snapshot.apps[1].upstream_port, None);
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
