//! Unit tests for agent metrics collection.

#[cfg(test)]
mod unit_tests {
    use crate::collector::{DeviceType, SystemCollector};

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
        assert_eq!(metrics.device_type, Some(DeviceType::Workstation));
        assert!((0.0..=100.0).contains(&metrics.cpu));
        assert!((0.0..=100.0).contains(&metrics.mem));
        // A host without a temperature sensor must report None, not a stand-in value.
        assert!(metrics.temp.is_none_or(|t| t > 0.0));

        // No power-health source is recognized unless the host actually is one
        // (e.g. this dev machine or a generic CI runner is not a Raspberry Pi),
        // and when there's no source, nothing is fabricated for the other two.
        if metrics.power_source.is_none() {
            assert_eq!(metrics.throttled, None);
            assert_eq!(metrics.under_voltage, None);
        }
    }

    #[test]
    fn test_power_detect_requires_recognized_hardware() {
        // On anything that isn't a Raspberry Pi (this dev machine, a generic CI
        // runner), detect() must return None rather than guessing.
        let is_pi = std::fs::read_to_string("/proc/device-tree/model")
            .or_else(|_| std::fs::read_to_string("/sys/firmware/devicetree/base/model"))
            .is_ok_and(|m| m.contains("Raspberry Pi"));

        if !is_pi {
            assert!(crate::power::detect().is_none());
        }
    }
}
