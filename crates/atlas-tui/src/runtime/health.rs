use std::sync::LazyLock;
use std::time::Duration;

use tokio::net::TcpStream;

/// One shared client: redirects are not followed (a 3xx already proves the app
/// is serving) and system proxies are ignored (health targets are local).
static HTTP: LazyLock<Option<reqwest::Client>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .ok()
});

/// Real HTTP probe for a service's `health:` URL: `GET`, healthy on 2xx/3xx.
/// The whole request is bounded by `timeout`. Connection errors, timeouts,
/// 4xx and 5xx are unhealthy. `https://` works (rustls).
pub async fn check_http(url: &str, timeout: Duration) -> bool {
    let Some(client) = HTTP.as_ref() else {
        return false;
    };
    match client.get(url).timeout(timeout).send().await {
        Ok(resp) => {
            let s = resp.status();
            s.is_success() || s.is_redirection()
        }
        Err(_) => false,
    }
}

pub async fn check_port(host: &str, port: u16, timeout: Duration) -> bool {
    tokio::time::timeout(timeout, TcpStream::connect((host, port)))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
}

/// One probe of a service's declared endpoint: its `health:` URL, else its port.
/// `None` when it declares neither (the caller decides what "alive" means then).
pub async fn probe(health_url: Option<&str>, port: Option<u16>, timeout: Duration) -> Option<bool> {
    if let Some(url) = health_url {
        Some(check_http(url, timeout).await)
    } else if let Some(port) = port {
        Some(check_port("127.0.0.1", port, timeout).await)
    } else {
        None
    }
}

/// Poll `host:port` every 500 ms until it accepts a connection or `timeout` expires.
/// Use this instead of `check_port` when a service may need time to start (e.g. postgres).
pub async fn wait_for_port(host: &str, port: u16, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if TcpStream::connect((host, port)).await.is_ok() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Poll until `host:port` stops accepting connections (port is free) or timeout expires.
/// Use before spawning a session that needs exclusive access to a port.
pub async fn wait_port_free(host: &str, port: u16, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if TcpStream::connect((host, port)).await.is_err() {
            return true; // port is free
        }
        if tokio::time::Instant::now() >= deadline {
            return false; // timed out waiting
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn check_process_alive(pid: u32) -> bool {
    use nix::sys::signal;
    use nix::unistd::Pid;
    signal::kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_check_port_localhost() {
        // Port 1 should be closed (reserved, no service running)
        let result = check_port("127.0.0.1", 1, Duration::from_millis(100)).await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_check_process_alive_self() {
        let pid = std::process::id();
        assert!(check_process_alive(pid).await);
    }

    #[tokio::test]
    async fn test_check_process_alive_nonexistent() {
        // PID 999999 almost certainly doesn't exist
        assert!(!check_process_alive(999_999).await);
    }

    /// Serve one canned status line per connection, in order; `None` = accept
    /// and never answer (to exercise the timeout).
    async fn serve(statuses: Vec<Option<&'static str>>) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            for status in statuses {
                let (mut sock, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    match status {
                        Some(line) => {
                            let resp = format!(
                                "HTTP/1.1 {line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            );
                            let _ = sock.write_all(resp.as_bytes()).await;
                            let _ = sock.shutdown().await;
                        }
                        None => tokio::time::sleep(Duration::from_secs(30)).await,
                    }
                });
            }
        });
        port
    }

    #[tokio::test]
    async fn http_unhealthy_on_500_then_healthy_on_200() {
        let port = serve(vec![Some("500 Internal Server Error"), Some("200 OK")]).await;
        let url = format!("http://127.0.0.1:{port}/health");
        let t = Duration::from_secs(2);
        assert!(!check_http(&url, t).await, "500 must be unhealthy");
        assert!(check_http(&url, t).await, "200 must be healthy");
    }

    #[tokio::test]
    async fn http_3xx_healthy_4xx_unhealthy_redirect_not_followed() {
        let port = serve(vec![
            Some("302 Found"),
            Some("404 Not Found"),
            Some("503 Service Unavailable"),
        ])
        .await;
        let url = format!("http://127.0.0.1:{port}/");
        let t = Duration::from_secs(2);
        assert!(check_http(&url, t).await, "302 counts as healthy");
        assert!(!check_http(&url, t).await, "404 is unhealthy");
        assert!(!check_http(&url, t).await, "503 is unhealthy");
    }

    #[tokio::test]
    async fn http_timeout_and_refused_are_unhealthy() {
        let port = serve(vec![None]).await;
        let url = format!("http://127.0.0.1:{port}/");
        let t0 = std::time::Instant::now();
        assert!(!check_http(&url, Duration::from_millis(300)).await);
        assert!(t0.elapsed() < Duration::from_secs(2));
        assert!(!check_http("http://127.0.0.1:1/", Duration::from_millis(300)).await);
        assert!(!check_http("not a url", Duration::from_millis(300)).await);
    }
}
