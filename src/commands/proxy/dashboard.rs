//! Read-only HTML dashboard for services managed by the PM orchestrator.
//!
//! `services.json` is the process registry. `routes.json` is joined only to
//! find a browser URL for each service, so canonical and short aliases never
//! create duplicate dashboard entries.

use crate::routes::{RouteEntry, RoutesData};
use crate::services::{self, ServicesData};
use bytes::Bytes;
use http_body_util::Full;
use hyper::header::{ALLOW, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE};
use hyper::{Method, Response, StatusCode};
use std::fmt::Write as _;

pub(super) const HOSTNAME: &str = "pm.localhost";

const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; \
     form-action 'none'; frame-ancestors 'none'";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppStatus {
    Running,
    RouteMissing,
    RouteMismatch,
    Stale,
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
    pid: u32,
    upstream_port: u16,
    started_at: String,
    url: Option<String>,
    status: AppStatus,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct DashboardSnapshot {
    apps: Vec<DashboardApp>,
    running_count: usize,
    attention_count: usize,
}

pub(super) fn is_dashboard_host(host: &str) -> bool {
    host.eq_ignore_ascii_case(HOSTNAME)
        || host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
}

pub(super) fn response(
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

    dashboard_response(method, routes, &services, proxy_port, services::pid_alive)
}

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
    let html = render_html(&snapshot, proxy_port);
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

fn build_snapshot<F>(
    routes: &RoutesData,
    services: &ServicesData,
    proxy_port: u16,
    mut is_alive: F,
) -> DashboardSnapshot
where
    F: FnMut(u32) -> bool,
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
                pid: state.pid,
                upstream_port: state.port,
                started_at: state.started_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                url,
                status,
            });
        }
    }

    apps.sort_by(|left, right| {
        (&left.workspace, &left.project, &left.service).cmp(&(
            &right.workspace,
            &right.project,
            &right.service,
        ))
    });
    let running_count = apps
        .iter()
        .filter(|app| app.status.is_process_running())
        .count();
    let attention_count = apps
        .iter()
        .filter(|app| !app.status.has_healthy_route())
        .count();

    DashboardSnapshot {
        apps,
        running_count,
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
            is_safe_route_hostname(hostname) && !is_dashboard_host(hostname.as_str())
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

fn render_html(snapshot: &DashboardSnapshot, proxy_port: u16) -> String {
    let dashboard_url = public_url(HOSTNAME, proxy_port);
    let mut cards = String::new();

    if snapshot.apps.is_empty() {
        cards.push_str(
            r#"<section class="empty">
  <div class="empty-icon">0</div>
  <h2>No PM applications are running</h2>
  <p>Start a configured service with <code>pm run</code> and it will appear here.</p>
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
  <title>PM Applications</title>
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
    .status {{ flex: 0 0 auto; border-radius: 999px; padding: 6px 9px; font-size: 11px; font-weight: 750; }}
    .status.running {{ color: #86efac; background: #14532d66; }}
    .status.warning {{ color: #fde68a; background: #78350f66; }}
    .status.stale {{ color: #d4d4d8; background: #3f3f4666; }}
    .url {{ display: block; overflow: hidden; margin: 20px 0; color: #93c5fd; font: 13px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; text-overflow: ellipsis; white-space: nowrap; text-decoration: none; }}
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
        <h1>PM Apps</h1>
      </div>
      <div class="summary">
        <span><strong>{running}</strong> running</span>
        <span><strong>{attention}</strong> need attention</span>
      </div>
    </header>
    <section class="grid">{cards}</section>
    <footer>
      <span>Read-only · refreshes every 5 seconds</span>
      <a href="{dashboard_url}">Refresh now</a>
    </footer>
  </main>
</body>
</html>"#,
        running = snapshot.running_count,
        attention = snapshot.attention_count,
        dashboard_url = escape_html(&dashboard_url),
    )
}

fn render_app(output: &mut String, app: &DashboardApp) {
    let workspace = escape_html(&app.workspace);
    let project = escape_html(&app.project);
    let service = escape_html(&app.service);
    let status = app.status;
    let url = app.url.as_deref().map(escape_html);

    let _ = write!(
        output,
        r#"<article class="card">
  <div class="card-head">
    <div class="identity">
      <div class="project">{workspace} / {project}</div>
      <h2>{service}</h2>
    </div>
    <span class="status {status_class}">{status_label}</span>
  </div>"#,
        status_class = status.css_class(),
        status_label = status.label(),
    );

    match url {
        Some(url) => {
            let _ = write!(output, r#"<a class="url" href="{url}">{url}</a>"#);
        }
        None => output.push_str(r#"<span class="url unavailable">Route unavailable</span>"#),
    }

    let _ = write!(
        output,
        r#"<dl>
    <div><dt>Port</dt><dd>{port}</dd></div>
    <div><dt>PID</dt><dd>{pid}</dd></div>
    <div><dt>Started</dt><dd>{started}</dd></div>
  </dl>
</article>"#,
        port = app.upstream_port,
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
        assert!(is_dashboard_host("pm.localhost"));
        assert!(is_dashboard_host("PM.LOCALHOST"));
        assert!(is_dashboard_host("localhost"));
        assert!(is_dashboard_host("127.0.0.1"));
        assert!(!is_dashboard_host("front.pm.localhost"));
        assert!(!is_dashboard_host("pm.localhost.evil"));
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
        assert_eq!(snapshot.running_count, 1);
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

        assert_eq!(snapshot.running_count, 2);
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

        assert_eq!(snapshot.running_count, 3);
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

        assert_eq!(snapshot.running_count, 1);
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
                pid: 1,
                upstream_port: 3000,
                started_at: "2026-07-25 12:30:00 UTC".into(),
                url: None,
                status: AppStatus::RouteMissing,
            }],
            running_count: 0,
            attention_count: 1,
        };

        let html = render_html(&snapshot, 7100);

        assert!(!html.contains("<script>alert"));
        assert!(!html.contains(r#"<img src=x>"#));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("api&amp;admin"));
        assert!(html.contains("&quot;&gt;&lt;img"));
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
        assert!(String::from_utf8_lossy(&get_body).contains("No PM applications are running"));

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

        let post = response(&Method::POST, "/", &routes, 7100);
        assert_eq!(post.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(post.headers().get(ALLOW).unwrap(), "GET, HEAD");

        let missing = response(&Method::GET, "/missing", &routes, 7100);
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let missing_head = response(&Method::HEAD, "/missing", &routes, 7100);
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
