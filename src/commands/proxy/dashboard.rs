//! Read-only dashboard API and embedded SPA for services managed by PM.
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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(super) const HOSTNAME: &str = "pm.localhost";

const DASHBOARD_SCHEMA_VERSION: u8 = 1;
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; img-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; \
     form-action 'none'; frame-ancestors 'none'";
const TAILSCALE_CACHE_TTL: Duration = Duration::from_secs(10);
const TAILSCALE_COMMAND_TIMEOUT: Duration = Duration::from_millis(500);
const DASHBOARD_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/dashboard-ui/dist/index.html"
));

#[derive(Clone, Copy)]
struct DashboardAsset {
    path: &'static str,
    content_type: &'static str,
    bytes: &'static [u8],
    cache_control: &'static str,
}

const DASHBOARD_ASSETS: &[DashboardAsset] = &[
    DashboardAsset {
        path: "/assets/dashboard/pm-dashboard.js",
        content_type: "text/javascript; charset=utf-8",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/dashboard-ui/dist/pm-dashboard.js"
        )),
        cache_control: "no-cache, must-revalidate",
    },
    DashboardAsset {
        path: "/assets/dashboard/pm-dashboard.css",
        content_type: "text/css; charset=utf-8",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/dashboard-ui/dist/pm-dashboard.css"
        )),
        cache_control: "no-cache, must-revalidate",
    },
    DashboardAsset {
        path: "/assets/dashboard/i-learn-card-v2.jpg",
        content_type: "image/jpeg",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/dashboard/i-learn-card-v2.jpg"
        )),
        cache_control: "public, max-age=31536000, immutable",
    },
    DashboardAsset {
        path: "/assets/dashboard/social-manager-card-v2.jpg",
        content_type: "image/jpeg",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/dashboard/social-manager-card-v2.jpg"
        )),
        cache_control: "public, max-age=31536000, immutable",
    },
    DashboardAsset {
        path: "/assets/dashboard/stock-analyzer-card-v2.jpg",
        content_type: "image/jpeg",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/dashboard/stock-analyzer-card-v2.jpg"
        )),
        cache_control: "public, max-age=31536000, immutable",
    },
    DashboardAsset {
        path: "/assets/dashboard/korion-card-v2.jpg",
        content_type: "image/jpeg",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/dashboard/korion-card-v2.jpg"
        )),
        cache_control: "public, max-age=31536000, immutable",
    },
    DashboardAsset {
        path: "/assets/dashboard/korion-studio-card-v2.jpg",
        content_type: "image/jpeg",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/dashboard/korion-studio-card-v2.jpg"
        )),
        cache_control: "public, max-age=31536000, immutable",
    },
    DashboardAsset {
        path: "/assets/dashboard/topclass-card-v2.jpg",
        content_type: "image/jpeg",
        bytes: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/dashboard/topclass-card-v2.jpg"
        )),
        cache_control: "public, max-age=31536000, immutable",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum AppStatus {
    Running,
    RouteMissing,
    RouteMismatch,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
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
}

impl AppStatus {
    fn is_process_running(self) -> bool {
        self != Self::Stale
    }

    fn has_healthy_route(self) -> bool {
        self == Self::Running
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[derive(Debug, PartialEq, Eq, Serialize)]
struct DashboardSnapshot {
    schema_version: u8,
    apps: Vec<DashboardApp>,
    background_count: usize,
    foreground_count: usize,
    attention_count: usize,
}

impl Default for DashboardSnapshot {
    fn default() -> Self {
        Self {
            schema_version: DASHBOARD_SCHEMA_VERSION,
            apps: Vec::new(),
            background_count: 0,
            foreground_count: 0,
            attention_count: 0,
        }
    }
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
    if let Some(asset) = dashboard_asset(path) {
        return dashboard_asset_response(method, asset);
    }

    match path {
        "/" => return dashboard_index_response(method),
        "/api/dashboard" => {}
        _ => {
            return plain_response(
                method,
                StatusCode::NOT_FOUND,
                "Dashboard resource not found",
                None,
            );
        }
    }

    if !is_read_method(method) {
        return method_not_allowed(method);
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
    dashboard_api_response(method, &snapshot)
}

fn dashboard_asset(path: &str) -> Option<DashboardAsset> {
    DASHBOARD_ASSETS
        .iter()
        .copied()
        .find(|asset| asset.path == path)
}

fn dashboard_asset_response(method: &Method, asset: DashboardAsset) -> Response<Full<Bytes>> {
    if !is_read_method(method) {
        return method_not_allowed(method);
    }

    let body = if method == Method::HEAD {
        Bytes::new()
    } else {
        Bytes::from_static(asset.bytes)
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, asset.content_type)
        .header(CACHE_CONTROL, asset.cache_control)
        .header(CONTENT_LENGTH, asset.bytes.len().to_string())
        .header("x-content-type-options", "nosniff")
        .body(Full::new(body))
        .expect("valid dashboard asset response")
}

fn dashboard_index_response(method: &Method) -> Response<Full<Bytes>> {
    if !is_read_method(method) {
        return method_not_allowed(method);
    }

    let body = if method == Method::HEAD {
        Bytes::new()
    } else {
        Bytes::from_static(DASHBOARD_INDEX)
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .header(CACHE_CONTROL, "no-cache, must-revalidate")
        .header(CONTENT_LENGTH, DASHBOARD_INDEX.len().to_string())
        .header("content-security-policy", CONTENT_SECURITY_POLICY)
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer")
        .body(Full::new(body))
        .expect("valid dashboard index response")
}

fn dashboard_api_response(method: &Method, snapshot: &DashboardSnapshot) -> Response<Full<Bytes>> {
    let json = serde_json::to_vec(snapshot).expect("dashboard snapshot is serializable");
    let content_length = json.len();
    let body = if method == Method::HEAD {
        Bytes::new()
    } else {
        Bytes::from(json)
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .header(CACHE_CONTROL, "no-store")
        .header(CONTENT_LENGTH, content_length.to_string())
        .header("x-content-type-options", "nosniff")
        .body(Full::new(body))
        .expect("valid dashboard API response")
}

fn is_read_method(method: &Method) -> bool {
    method == Method::GET || method == Method::HEAD
}

fn method_not_allowed(method: &Method) -> Response<Full<Bytes>> {
    plain_response(
        method,
        StatusCode::METHOD_NOT_ALLOWED,
        "Dashboard is read-only",
        Some("GET, HEAD"),
    )
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
        schema_version: DASHBOARD_SCHEMA_VERSION,
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
        let index_get = response(&Method::GET, "/", &routes, 7100).await;
        assert_eq!(index_get.status(), StatusCode::OK);
        assert_eq!(
            index_get.headers().get(CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            index_get.headers().get(CACHE_CONTROL).unwrap(),
            "no-cache, must-revalidate"
        );
        let content_security_policy = index_get
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_security_policy.contains("img-src 'self'"));
        assert!(content_security_policy.contains("script-src 'self'"));
        assert!(content_security_policy.contains("style-src 'self'"));
        assert!(content_security_policy.contains("connect-src 'self'"));
        assert!(!content_security_policy.contains("'unsafe-inline'"));
        let index_length = index_get.headers().get(CONTENT_LENGTH).unwrap().clone();
        let index_body = index_get.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(index_body.as_ref(), DASHBOARD_INDEX);

        let index_head = response(&Method::HEAD, "/", &routes, 7100).await;
        assert_eq!(index_head.status(), StatusCode::OK);
        assert_eq!(
            index_head.headers().get(CONTENT_LENGTH).unwrap(),
            index_length
        );
        assert!(
            index_head
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );

        let post = response(&Method::POST, "/", &routes, 7100).await;
        assert_eq!(post.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(post.headers().get(ALLOW).unwrap(), "GET, HEAD");

        for (path, content_type) in [
            (
                "/assets/dashboard/pm-dashboard.js",
                "text/javascript; charset=utf-8",
            ),
            (
                "/assets/dashboard/pm-dashboard.css",
                "text/css; charset=utf-8",
            ),
        ] {
            let asset_get = response(&Method::GET, path, &routes, 7100).await;
            assert_eq!(asset_get.status(), StatusCode::OK);
            assert_eq!(asset_get.headers().get(CONTENT_TYPE).unwrap(), content_type);
            assert_eq!(
                asset_get.headers().get(CACHE_CONTROL).unwrap(),
                "no-cache, must-revalidate"
            );
            let asset_length = asset_get.headers().get(CONTENT_LENGTH).unwrap().clone();
            assert!(
                !asset_get
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes()
                    .is_empty()
            );

            let asset_head = response(&Method::HEAD, path, &routes, 7100).await;
            assert_eq!(asset_head.status(), StatusCode::OK);
            assert_eq!(
                asset_head.headers().get(CONTENT_LENGTH).unwrap(),
                asset_length
            );
            assert!(
                asset_head
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes()
                    .is_empty()
            );

            let asset_post = response(&Method::POST, path, &routes, 7100).await;
            assert_eq!(asset_post.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_eq!(asset_post.headers().get(ALLOW).unwrap(), "GET, HEAD");
        }

        let image_path = "/assets/dashboard/social-manager-card-v2.jpg";
        let image_get = response(&Method::GET, image_path, &routes, 7100).await;
        assert_eq!(image_get.status(), StatusCode::OK);
        assert_eq!(
            image_get.headers().get(CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(image_get.headers().get(CONTENT_TYPE).unwrap(), "image/jpeg");
        assert!(
            image_get
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .len()
                > 1_000
        );

        let retired_refresh = response(
            &Method::GET,
            "/assets/dashboard/refresh-v1.js",
            &routes,
            7100,
        )
        .await;
        assert_eq!(retired_refresh.status(), StatusCode::NOT_FOUND);

        let api_post = response(&Method::POST, "/api/dashboard", &routes, 7100).await;
        assert_eq!(api_post.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(api_post.headers().get(ALLOW).unwrap(), "GET, HEAD");

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

    #[tokio::test]
    async fn dashboard_api_serializes_only_the_versioned_read_model() {
        let snapshot = DashboardSnapshot {
            schema_version: DASHBOARD_SCHEMA_VERSION,
            apps: vec![
                DashboardApp {
                    workspace: "personal".into(),
                    project: "social-manager".into(),
                    service: "front".into(),
                    kind: Some(PortKind::Frontend),
                    mode: ProcessMode::Background,
                    pid: 42,
                    upstream_port: Some(17_790),
                    started_at: "2026-07-25 12:30:00 UTC".into(),
                    url: Some("http://front.social-manager.personal.localhost:7100/".into()),
                    tailnet_urls: vec!["https://m5-air.tail4beb79.ts.net:14443/".into()],
                    status: AppStatus::Running,
                },
                DashboardApp {
                    workspace: "personal".into(),
                    project: "worker".into(),
                    service: "pnpm".into(),
                    kind: None,
                    mode: ProcessMode::Foreground,
                    pid: 77,
                    upstream_port: None,
                    started_at: "2026-07-25 12:31:00 UTC".into(),
                    url: None,
                    tailnet_urls: Vec::new(),
                    status: AppStatus::Stale,
                },
            ],
            background_count: 1,
            foreground_count: 0,
            attention_count: 1,
        };

        let get = dashboard_api_response(&Method::GET, &snapshot);
        assert_eq!(get.status(), StatusCode::OK);
        assert_eq!(
            get.headers().get(CONTENT_TYPE).unwrap(),
            "application/json; charset=utf-8"
        );
        assert_eq!(get.headers().get(CACHE_CONTROL).unwrap(), "no-store");
        let get_length = get.headers().get(CONTENT_LENGTH).unwrap().clone();
        let body = get.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["background_count"], 1);
        assert_eq!(value["foreground_count"], 0);
        assert_eq!(value["attention_count"], 1);
        let root_fields = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            root_fields,
            [
                "schema_version",
                "apps",
                "background_count",
                "foreground_count",
                "attention_count",
            ]
            .into_iter()
            .collect()
        );

        let app = value["apps"][0].as_object().unwrap();
        let app_fields = app
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            app_fields,
            [
                "workspace",
                "project",
                "service",
                "kind",
                "mode",
                "pid",
                "upstream_port",
                "started_at",
                "url",
                "tailnet_urls",
                "status",
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(app["kind"], "frontend");
        assert_eq!(app["mode"], "background");
        assert_eq!(app["status"], "running");
        assert_eq!(
            app["tailnet_urls"][0],
            "https://m5-air.tail4beb79.ts.net:14443/"
        );
        assert_eq!(value["apps"][1]["kind"], serde_json::Value::Null);
        assert_eq!(value["apps"][1]["mode"], "foreground");
        assert_eq!(value["apps"][1]["status"], "stale");
        for unsafe_field in [
            "log_path",
            "dev_cmd",
            "start_token",
            "command",
            "environment",
        ] {
            assert!(
                !body
                    .windows(unsafe_field.len())
                    .any(|window| window == unsafe_field.as_bytes())
            );
        }

        let head = dashboard_api_response(&Method::HEAD, &snapshot);
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.headers().get(CONTENT_LENGTH).unwrap(), get_length);
        assert!(
            head.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );
    }
}
