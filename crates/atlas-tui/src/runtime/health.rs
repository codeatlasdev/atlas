use std::time::Duration;

use tokio::net::TcpStream;

pub async fn check_http(url: &str, timeout: Duration) -> bool {
    let host_port = match parse_host_port(url) {
        Some(hp) => hp,
        None => return false,
    };
    tokio::time::timeout(timeout, TcpStream::connect(host_port.as_str()))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
}

pub async fn check_port(host: &str, port: u16, timeout: Duration) -> bool {
    tokio::time::timeout(timeout, TcpStream::connect((host, port)))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
}

pub async fn check_process_alive(pid: u32) -> bool {
    use nix::sys::signal;
    use nix::unistd::Pid;
    signal::kill(Pid::from_raw(pid as i32), None).is_ok()
}

fn parse_host_port(url: &str) -> Option<String> {
    // Strip scheme
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);

    // Take host:port (before any path)
    let authority = without_scheme.split('/').next()?;

    if authority.contains(':') {
        Some(authority.to_string())
    } else {
        // Default ports
        if url.starts_with("https://") {
            Some(format!("{authority}:443"))
        } else {
            Some(format!("{authority}:80"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_parse_host_port_with_port() {
        assert_eq!(
            parse_host_port("http://localhost:3000/health"),
            Some("localhost:3000".to_string())
        );
    }

    #[test]
    fn test_parse_host_port_default_http() {
        assert_eq!(
            parse_host_port("http://example.com/path"),
            Some("example.com:80".to_string())
        );
    }

    #[test]
    fn test_parse_host_port_default_https() {
        assert_eq!(
            parse_host_port("https://example.com/path"),
            Some("example.com:443".to_string())
        );
    }
}
