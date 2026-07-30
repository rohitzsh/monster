//! Unit tests for agent metrics collection.

#[cfg(test)]
mod unit_tests {
    use crate::collector::SystemCollector;

    #[test]
    fn test_collector_sampling() {
        let mut collector = SystemCollector::new(
            Some("test-id-123".to_string()),
            Some("test-agent".to_string()),
            "workstation".to_string(),
        );

        let metrics = collector.sample();
        assert_eq!(metrics.device_id, "test-id-123");
        assert_eq!(metrics.name, "test-agent");
        assert_eq!(metrics.device_type, "workstation");
        assert!((0.0..=100.0).contains(&metrics.cpu));
        assert!((0.0..=100.0).contains(&metrics.mem));
    }
}
