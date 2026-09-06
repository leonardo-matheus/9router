use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, error, info};

/// Health checker for the 9Router server (TCP-only to keep dependencies minimal)
#[derive(Debug, Clone)]
pub struct HealthChecker {
    address: SocketAddr,
}

impl HealthChecker {
    pub fn new(address: SocketAddr) -> Self {
        Self { address }
    }

    /// Check if the server is responding on the TCP port using async I/O
    pub async fn check_tcp(&self, timeout_duration: Duration) -> bool {
        match timeout(timeout_duration, TcpStream::connect(self.address)).await {
            Ok(Ok(_)) => true,
            Ok(Err(e)) => {
                debug!("TCP connection failed: {}", e);
                false
            }
            Err(_) => {
                debug!("TCP connection timed out");
                false
            }
        }
    }

    /// Perform a single health check
    pub async fn check_now(&self) -> bool {
        self.check_tcp(Duration::from_secs(2)).await
    }

    /// Wait until the server is ready (returns error on timeout so caller can retry)
    pub async fn wait_ready(&self, timeout_duration: Duration) -> anyhow::Result<()> {
        info!("Waiting for server to become ready (timeout: {:?})...", timeout_duration);

        let start = Instant::now();
        let check_interval = Duration::from_millis(500);

        loop {
            if start.elapsed() > timeout_duration {
                error!("Server did not become ready within {:?}", timeout_duration);
                anyhow::bail!("Server startup timeout exceeded");
            }

            if self.check_tcp(Duration::from_secs(2)).await {
                let elapsed = start.elapsed();
                info!("Server ready after {:?}", elapsed);
                return Ok(());
            }

            debug!("Server not ready yet, retrying...");
            tokio::time::sleep(check_interval).await;
        }
    }

    /// Monitor server health until failure (returns error so caller can retry)
    pub async fn monitor_until_failure(&self, check_interval: Duration) -> anyhow::Result<()> {
        info!("Monitoring server health (check every {:?})", check_interval);

        loop {
            tokio::time::sleep(check_interval).await;

            if !self.check_tcp(Duration::from_secs(2)).await {
                error!("Server stopped responding");
                anyhow::bail!("Server health check failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_checker_creation() {
        let addr: SocketAddr = "127.0.0.1:20128".parse().unwrap();
        let _checker = HealthChecker::new(addr);
    }

    #[tokio::test]
    async fn test_tcp_check_timeout() {
        let addr: SocketAddr = "127.0.0.1:19999".parse().unwrap();
        let checker = HealthChecker::new(addr);
        let result = checker.check_tcp(Duration::from_millis(100)).await;
        assert!(!result);
    }
}
