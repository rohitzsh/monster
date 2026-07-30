//! Pluggable power/throttle health monitoring.
//!
//! Not every device can report this, and the devices that can use very
//! different mechanisms (Raspberry Pi firmware flags, Linux RAPL energy
//! counters, laptop battery drivers, IPMI power supplies...). `PowerMonitor`
//! is the seam: `detect()` probes the host once at startup and returns the
//! first implementation that recognizes the hardware, or `None` if none does.

/// A single power-health sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerReading {
    /// Some active power-related constraint is in effect right now (frequency
    /// capped, throttled, or under-voltage).
    pub throttled: bool,
    /// An under-voltage condition is active right now.
    pub under_voltage: bool,
}

/// A platform-specific source of power health data.
pub trait PowerMonitor: Send {
    /// Short identifier reported to the server as `power_source`, e.g. `"raspberry-pi"`.
    fn name(&self) -> &'static str;

    /// Sample current power health. `None` if the read failed this cycle.
    fn sample(&mut self) -> Option<PowerReading>;
}

/// Probe the host for a supported power monitor.
///
/// Returns `None` when no implementation recognizes the hardware. Add new
/// sources here (RAPL, `battery`, IPMI, ...) without touching any caller.
pub fn detect() -> Option<Box<dyn PowerMonitor>> {
    raspberry_pi::RaspberryPiPowerMonitor::detect().map(|m| Box::new(m) as Box<dyn PowerMonitor>)
}

mod raspberry_pi {
    use super::{PowerMonitor, PowerReading};
    use std::process::Command;

    /// Bit 0: under-voltage detected right now.
    const UNDER_VOLTAGE_NOW: u32 = 0x1;
    /// Bit 1: ARM frequency capped right now.
    const FREQ_CAPPED_NOW: u32 = 0x2;
    /// Bit 2: currently throttled.
    const THROTTLED_NOW: u32 = 0x4;

    const SYSFS_THROTTLED: &str = "/sys/devices/platform/soc/soc:firmware/get_throttled";

    /// Reads the Raspberry Pi firmware's `get_throttled` bitmask.
    pub struct RaspberryPiPowerMonitor {
        /// Whether the sysfs attribute is usable; otherwise fall back to `vcgencmd`.
        use_sysfs: bool,
    }

    impl RaspberryPiPowerMonitor {
        /// Identify a Raspberry Pi by its device-tree model string.
        ///
        /// Both reads simply fail on non-Linux or non-Pi hosts, so this needs
        /// no `cfg(target_os = ...)` gating.
        pub fn detect() -> Option<Self> {
            let model = std::fs::read_to_string("/proc/device-tree/model")
                .or_else(|_| std::fs::read_to_string("/sys/firmware/devicetree/base/model"))
                .ok()?;
            if !model.contains("Raspberry Pi") {
                return None;
            }

            let use_sysfs = std::path::Path::new(SYSFS_THROTTLED).exists();
            if !use_sysfs && !Self::vcgencmd_available() {
                // Board detected but neither read path works (e.g. inside a
                // container without /sys mounted, or vcgencmd not installed).
                return None;
            }
            Some(Self { use_sysfs })
        }

        fn vcgencmd_available() -> bool {
            Command::new("vcgencmd")
                .arg("get_throttled")
                .output()
                .is_ok_and(|o| o.status.success())
        }

        fn read_bits(&self) -> Option<u32> {
            let raw = if self.use_sysfs {
                std::fs::read_to_string(SYSFS_THROTTLED).ok()?
            } else {
                let output = Command::new("vcgencmd")
                    .arg("get_throttled")
                    .output()
                    .ok()?;
                String::from_utf8(output.stdout).ok()?
            };
            parse_throttled(&raw)
        }
    }

    /// Parse `"0x50005"` or `"throttled=0x50005"` into the raw bitmask.
    fn parse_throttled(raw: &str) -> Option<u32> {
        let hex = raw.trim().rsplit('=').next()?.trim();
        u32::from_str_radix(hex.trim_start_matches("0x"), 16).ok()
    }

    impl PowerMonitor for RaspberryPiPowerMonitor {
        fn name(&self) -> &'static str {
            "raspberry-pi"
        }

        fn sample(&mut self) -> Option<PowerReading> {
            let bits = self.read_bits()?;
            Some(PowerReading {
                under_voltage: bits & UNDER_VOLTAGE_NOW != 0,
                throttled: bits & (UNDER_VOLTAGE_NOW | FREQ_CAPPED_NOW | THROTTLED_NOW) != 0,
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_bare_hex() {
            assert_eq!(parse_throttled("0x50005\n"), Some(0x50005));
        }

        #[test]
        fn parses_vcgencmd_output() {
            assert_eq!(parse_throttled("throttled=0x50005\n"), Some(0x50005));
        }

        #[test]
        fn bitmask_reads_current_state_only() {
            // Bit 16 (0x10000) is "has occurred since boot" and must not be
            // read as a currently-active condition.
            let historical_only = 0x10000;
            assert_eq!(historical_only & UNDER_VOLTAGE_NOW, 0);
            assert_eq!(
                historical_only & (UNDER_VOLTAGE_NOW | FREQ_CAPPED_NOW | THROTTLED_NOW),
                0
            );
        }
    }
}
