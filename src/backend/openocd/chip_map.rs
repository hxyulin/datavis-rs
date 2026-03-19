/// Map a chip name to an OpenOCD target config file name.
/// Uses prefix matching (case-insensitive).
pub fn chip_to_target(chip_name: &str) -> Option<&'static str> {
    let upper = chip_name.to_uppercase();

    // STM32 families
    if upper.starts_with("STM32F0") { return Some("stm32f0x"); }
    if upper.starts_with("STM32F1") { return Some("stm32f1x"); }
    if upper.starts_with("STM32F2") { return Some("stm32f2x"); }
    if upper.starts_with("STM32F3") { return Some("stm32f3x"); }
    if upper.starts_with("STM32F4") { return Some("stm32f4x"); }
    if upper.starts_with("STM32F7") { return Some("stm32f7x"); }
    if upper.starts_with("STM32G0") { return Some("stm32g0x"); }
    if upper.starts_with("STM32G4") { return Some("stm32g4x"); }
    if upper.starts_with("STM32H7") { return Some("stm32h7x"); }
    if upper.starts_with("STM32L0") { return Some("stm32l0x"); }
    if upper.starts_with("STM32L1") { return Some("stm32l1"); }
    if upper.starts_with("STM32L4") { return Some("stm32l4x"); }
    if upper.starts_with("STM32L5") { return Some("stm32l5x"); }
    if upper.starts_with("STM32U5") { return Some("stm32u5x"); }
    if upper.starts_with("STM32WB") { return Some("stm32wbx"); }
    if upper.starts_with("STM32WL") { return Some("stm32wlx"); }

    // Nordic
    if upper.starts_with("NRF52") { return Some("nrf52"); }
    if upper.starts_with("NRF53") { return Some("nrf5340"); }

    // Raspberry Pi
    if upper.starts_with("RP2040") { return Some("rp2040"); }
    if upper.starts_with("RP2350") { return Some("rp2350"); }

    None
}

/// Map a probe type string to an OpenOCD interface config name.
pub fn probe_type_to_interface(probe_type: &str) -> Option<&'static str> {
    let lower = probe_type.to_lowercase();

    if lower.contains("stlink") || lower.contains("st-link") {
        return Some("stlink");
    }
    if lower.contains("cmsis-dap") || lower.contains("cmsisdap") || lower.contains("daplink") {
        return Some("cmsis-dap");
    }
    if lower.contains("jlink") || lower.contains("j-link") {
        return Some("jlink");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chip_to_target_stm32() {
        assert_eq!(chip_to_target("STM32F407VGTx"), Some("stm32f4x"));
        assert_eq!(chip_to_target("STM32F103C8"), Some("stm32f1x"));
        assert_eq!(chip_to_target("STM32H743ZI"), Some("stm32h7x"));
        assert_eq!(chip_to_target("STM32L476RG"), Some("stm32l4x"));
        assert_eq!(chip_to_target("STM32G431KB"), Some("stm32g4x"));
    }

    #[test]
    fn test_chip_to_target_case_insensitive() {
        assert_eq!(chip_to_target("stm32f407vgtx"), Some("stm32f4x"));
        assert_eq!(chip_to_target("Stm32F407VGTx"), Some("stm32f4x"));
    }

    #[test]
    fn test_chip_to_target_nordic() {
        assert_eq!(chip_to_target("nRF52840_xxAA"), Some("nrf52"));
        assert_eq!(chip_to_target("nRF5340_xxAA"), Some("nrf5340"));
    }

    #[test]
    fn test_chip_to_target_rp() {
        assert_eq!(chip_to_target("RP2040"), Some("rp2040"));
    }

    #[test]
    fn test_chip_to_target_unknown() {
        assert_eq!(chip_to_target("ESP32"), None);
        assert_eq!(chip_to_target("UNKNOWN_CHIP"), None);
    }

    #[test]
    fn test_probe_type_to_interface() {
        assert_eq!(probe_type_to_interface("STLink"), Some("stlink"));
        assert_eq!(probe_type_to_interface("ST-Link V2"), Some("stlink"));
        assert_eq!(probe_type_to_interface("CMSIS-DAP"), Some("cmsis-dap"));
        assert_eq!(probe_type_to_interface("DAPLink"), Some("cmsis-dap"));
        assert_eq!(probe_type_to_interface("J-Link"), Some("jlink"));
    }

    #[test]
    fn test_probe_type_unknown() {
        assert_eq!(probe_type_to_interface("Unknown Probe"), None);
    }
}
