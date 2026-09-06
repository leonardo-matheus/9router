use nine_router_bootstrap::process::ProcessManager;
use nine_router_bootstrap::retry::RetryPolicy;

#[tokio::test]
async fn test_find_node_success() {
    // This test verifies Node.js can be found
    let result = ProcessManager::find_node().await;
    assert!(result.is_ok(), "Node.js should be found in test environment");
    let node_path = result.unwrap();
    assert!(node_path.contains("node"), "Found path should contain 'node'");
}

#[tokio::test]
async fn test_process_manager_creation() {
    let manager = ProcessManager::new(
        "/usr/bin/node".to_string(),
        "--max-old-space-size=6144".to_string(),
        "/path/to/server.js".to_string(),
        vec!["--port".to_string(), "20128".to_string()],
    );

    assert_eq!(manager.node_path, "/usr/bin/node");
    assert_eq!(manager.server_path, "/path/to/server.js");
}

#[tokio::test]
async fn test_retry_policy_default() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.max_attempts(), 3);
    assert_eq!(policy.current_attempt(), 0);
    assert!(policy.should_retry());
}

#[tokio::test]
async fn test_retry_policy_with_custom_values() {
    let mut policy = RetryPolicy::new(5, 500);

    assert_eq!(policy.max_attempts(), 5);

    // Test retry sequence
    for _ in 1..=5 {
        let delay = policy.next_delay();
        assert!(delay >= std::time::Duration::from_millis(500));
    }

    // Should not retry after max attempts
    assert!(!policy.should_retry());
}

#[tokio::test]
async fn test_retry_policy_reset() {
    let mut policy = RetryPolicy::new(3, 1000);

    let _ = policy.next_delay();
    let _ = policy.next_delay();
    assert_eq!(policy.current_attempt(), 2);

    policy.reset();
    assert_eq!(policy.current_attempt(), 0);
    assert!(policy.should_retry());
}

#[tokio::test]
async fn test_retry_policy_exponential_backoff() {
    let mut policy = RetryPolicy::new(5, 1000);

    let delay1 = policy.next_delay();
    let delay2 = policy.next_delay();
    let delay3 = policy.next_delay();

    // Delays should increase (exponential backoff)
    assert!(delay2 > delay1, "Second delay should be larger than first");
    assert!(delay3 > delay2, "Third delay should be larger than second");
}

#[tokio::test]
async fn test_spawn_nonexistent_server() {
    let manager = ProcessManager::new(
        "/usr/bin/node".to_string(),
        String::new(),
        "/nonexistent/path/to/server.js".to_string(),
        Vec::new(),
    );

    let result = manager.spawn().await;
    // spawn() succeeds because it only checks process creation, not script existence
    // Script validation is done separately before calling spawn()
    assert!(result.is_ok());
}
