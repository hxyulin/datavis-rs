//! Bulk read manager for optimizing memory reads
//!
//! This module provides optimization for reading multiple variables by grouping
//! adjacent addresses into single larger reads. This reduces probe communication
//! overhead significantly when variables are located close together in memory.
//!
//! Also provides DependentReadPlanner for two-stage pointer dereferencing:
//! 1. Read pointer values at lower rate (e.g., 1 Hz)
//! 2. Read pointed-to data at normal rate using cached pointer addresses
//!
//! # Example
//!
//! ```ignore
//! use datavis_rs::backend::read_manager::ReadManager;
//!
//! let manager = ReadManager::new(64);  // 64-byte gap threshold
//! let regions = manager.plan_reads(&variables);
//!
//! // Now read each region and extract values
//! for region in regions {
//!     let data = probe.read_memory(region.address, region.size)?;
//!     for &idx in &region.variable_indices {
//!         let value = manager.extract_value(&variables[idx], &region, &data);
//!     }
//! }
//! ```

use crate::types::{Variable, PointerState};
use std::time::Instant;

/// Default gap threshold for combining reads (64 bytes)
pub const DEFAULT_GAP_THRESHOLD: usize = 64;

/// Manages bulk memory reads for better efficiency
///
/// Groups variables with adjacent addresses into single larger reads
/// to reduce probe communication overhead.
#[derive(Debug, Clone)]
pub struct ReadManager {
    /// Maximum gap between addresses to combine into single read
    gap_threshold: usize,
}

/// A planned memory read region
///
/// Represents a contiguous memory region to read that covers one or more variables.
#[derive(Debug, Clone)]
pub struct ReadRegion {
    /// Start address of the read
    pub address: u64,
    /// Number of bytes to read
    pub size: usize,
    /// Variable indices (from the original slice) that fall within this region
    pub variable_indices: Vec<usize>,
}

impl ReadManager {
    /// Create a new read manager with the specified gap threshold
    ///
    /// # Arguments
    /// * `gap_threshold` - Maximum gap in bytes between variable addresses to combine
    ///   into a single read. Larger values create bigger reads but may read unnecessary
    ///   memory. Default is 64 bytes.
    pub fn new(gap_threshold: usize) -> Self {
        Self { gap_threshold }
    }

    /// Get the current gap threshold
    pub fn gap_threshold(&self) -> usize {
        self.gap_threshold
    }

    /// Plan optimized reads for a set of variables
    ///
    /// Analyzes the variable addresses and groups them into read regions.
    /// Variables within `gap_threshold` bytes of each other are combined
    /// into a single larger read.
    ///
    /// # Arguments
    /// * `variables` - Slice of variables to plan reads for
    ///
    /// # Returns
    /// A vector of `ReadRegion`s that cover all variables, optimized to minimize
    /// the number of separate read operations.
    pub fn plan_reads(&self, variables: &[Variable]) -> Vec<ReadRegion> {
        if variables.is_empty() {
            return Vec::new();
        }

        // Create (index, address, size) tuples and sort by address
        let mut indexed: Vec<(usize, u64, usize)> = variables
            .iter()
            .enumerate()
            .map(|(i, v)| (i, v.address, v.var_type.size_bytes()))
            .collect();
        indexed.sort_by_key(|&(_, addr, _)| addr);

        let mut regions = Vec::new();
        let mut current_start = indexed[0].1;
        let mut current_end = indexed[0].1 + indexed[0].2 as u64;
        let mut current_indices = vec![indexed[0].0];

        for &(idx, addr, size) in &indexed[1..] {
            let var_end = addr + size as u64;

            // Check if this variable can be merged with current region
            // We merge if the variable starts within the gap threshold of the current end
            if addr <= current_end + self.gap_threshold as u64 {
                // Extend current region
                current_end = current_end.max(var_end);
                current_indices.push(idx);
            } else {
                // Start new region
                regions.push(ReadRegion {
                    address: current_start,
                    size: (current_end - current_start) as usize,
                    variable_indices: current_indices,
                });
                current_start = addr;
                current_end = var_end;
                current_indices = vec![idx];
            }
        }

        // Push final region
        regions.push(ReadRegion {
            address: current_start,
            size: (current_end - current_start) as usize,
            variable_indices: current_indices,
        });

        regions
    }

    /// Extract a variable's value from bulk read data
    ///
    /// After reading a region's data, use this to parse individual variable values.
    ///
    /// # Arguments
    /// * `variable` - The variable to extract
    /// * `region` - The read region the data came from
    /// * `data` - The raw bytes read from the region
    ///
    /// # Returns
    /// The parsed value if the variable falls within the region and data is valid,
    /// or `None` if the variable is outside the region or parsing fails.
    pub fn extract_value(&self, variable: &Variable, region: &ReadRegion, data: &[u8]) -> Option<f64> {
        // Check if variable address is within the region
        if variable.address < region.address {
            return None;
        }

        let offset = (variable.address - region.address) as usize;
        let size = variable.var_type.size_bytes();

        // Check if we have enough data
        if offset + size > data.len() {
            return None;
        }

        // Parse the value from the appropriate offset
        variable.var_type.parse_to_f64(&data[offset..offset + size])
    }

    /// Calculate how many individual reads would be saved by using bulk reads
    ///
    /// # Arguments
    /// * `variables` - The variables to analyze
    ///
    /// # Returns
    /// A tuple of (bulk_reads, individual_reads_saved)
    pub fn calculate_savings(&self, variables: &[Variable]) -> (usize, usize) {
        if variables.is_empty() {
            return (0, 0);
        }

        let regions = self.plan_reads(variables);
        let bulk_reads = regions.len();
        let individual_reads = variables.len();
        let saved = individual_reads.saturating_sub(bulk_reads);

        (bulk_reads, saved)
    }
}

impl Default for ReadManager {
    fn default() -> Self {
        Self::new(DEFAULT_GAP_THRESHOLD)
    }
}

/// Manages two-stage reads for pointer dereferencing
///
/// Separates pointer variables (read at lower rate) from dependent variables
/// (data pointed to, read at normal rate). Resolves dependent addresses using
/// cached pointer values.
#[derive(Debug, Default)]
pub struct DependentReadPlanner {
    /// Last time each pointer was read (keyed by variable ID)
    last_pointer_reads: std::collections::HashMap<u32, Instant>,
}

impl DependentReadPlanner {
    /// Create a new dependent read planner
    pub fn new() -> Self {
        Self {
            last_pointer_reads: std::collections::HashMap::new(),
        }
    }

    /// Plan two-stage reads: pointers first, then data
    ///
    /// # Arguments
    /// * `all_vars` - All enabled variables to consider
    ///
    /// # Returns
    /// A tuple of (pointers_to_read, data_to_read) where:
    /// - pointers_to_read: Pointer variables that need their value updated
    /// - data_to_read: All variables with resolved addresses (non-pointers + cached pointers)
    pub fn plan_reads(&mut self, all_vars: &[Variable]) -> (Vec<Variable>, Vec<Variable>) {
        let now = Instant::now();
        let mut pointers_to_read = Vec::new();
        let mut data_to_read = Vec::new();

        for var in all_vars {
            if let Some(ptr_meta) = &var.pointer_metadata {
                // This is a pointer variable or depends on a pointer
                if ptr_meta.pointer_parent_id.is_none() {
                    // This is the pointer itself (not a dependent child)
                    if self.should_read_pointer(var, now) {
                        pointers_to_read.push(var.clone());
                    }
                    // Always add to data reads (we'll use cached address or current value)
                    data_to_read.push(var.clone());
                } else {
                    // This is a dependent variable - will be resolved later
                    // For now, just add with its current address
                    data_to_read.push(var.clone());
                }
            } else {
                // Regular variable (non-pointer)
                data_to_read.push(var.clone());
            }
        }

        (pointers_to_read, data_to_read)
    }

    /// Check if a pointer variable needs to be read based on its poll rate
    fn should_read_pointer(&self, var: &Variable, now: Instant) -> bool {
        let Some(ptr_meta) = &var.pointer_metadata else {
            return false;
        };

        // If never read, always read
        let Some(last_read) = self.last_pointer_reads.get(&var.id) else {
            return true;
        };

        // Check if enough time has passed based on poll rate
        let poll_rate_hz = ptr_meta.pointer_poll_rate_hz.max(1);
        let interval = std::time::Duration::from_secs_f64(1.0 / poll_rate_hz as f64);
        now.duration_since(*last_read) >= interval
    }

    /// Update pointer cache after reading pointer values
    ///
    /// # Arguments
    /// * `variables` - The variables that were read (with updated pointer_metadata)
    ///
    /// This should be called after successfully reading pointer values to update
    /// the timestamp tracking.
    pub fn update_pointer_cache(&mut self, variables: &[Variable]) {
        let now = Instant::now();
        for var in variables {
            if let Some(ptr_meta) = &var.pointer_metadata {
                if ptr_meta.pointer_parent_id.is_none() {
                    // This is a pointer variable (not a dependent)
                    self.last_pointer_reads.insert(var.id, now);
                }
            }
        }
    }

    /// Resolve dependent variable addresses using cached pointer values
    ///
    /// # Arguments
    /// * `variables` - All variables (includes both pointers and dependents)
    ///
    /// # Returns
    /// A new vector with dependent variables having their addresses resolved
    /// to pointer_value + offset. Variables without valid pointer parents are unchanged.
    pub fn resolve_addresses(&self, variables: &[Variable]) -> Vec<Variable> {
        // Build a map of pointer ID -> cached address
        let mut pointer_addresses = std::collections::HashMap::new();
        for var in variables {
            if let Some(ptr_meta) = &var.pointer_metadata {
                if ptr_meta.pointer_parent_id.is_none() {
                    // This is a pointer, store its cached address
                    if let Some(cached_addr) = ptr_meta.cached_address {
                        pointer_addresses.insert(var.id, cached_addr);
                    }
                }
            }
        }

        // Resolve dependent variable addresses
        variables.iter().map(|var| {
            if let Some(ptr_meta) = &var.pointer_metadata {
                if let Some(parent_id) = ptr_meta.pointer_parent_id {
                    // This is a dependent variable
                    if let Some(&parent_addr) = pointer_addresses.get(&parent_id) {
                        // Resolve: parent_address + offset
                        let mut resolved = var.clone();
                        resolved.address = parent_addr.wrapping_add(ptr_meta.offset_from_pointer);
                        return resolved;
                    }
                }
            }
            var.clone()
        }).collect()
    }

    /// Update pointer state based on read value
    ///
    /// # Arguments
    /// * `variable` - The variable to update (must have pointer_metadata)
    /// * `value` - The read value (interpreted as address)
    ///
    /// # Returns
    /// Updated variable with pointer state set appropriately
    pub fn update_pointer_state(variable: &mut Variable, value: f64) {
        let Some(ptr_meta) = &mut variable.pointer_metadata else {
            return;
        };

        let addr = value as u64;

        // Update cached address
        ptr_meta.cached_address = Some(addr);

        // Determine pointer state
        ptr_meta.pointer_state = if addr == 0 {
            PointerState::Null
        } else if addr < 0x1000 || addr > 0xFFFF_FFFF_0000_0000 {
            // Suspicious addresses (very low or very high)
            PointerState::Invalid(addr)
        } else if addr % 4 != 0 {
            // Unaligned pointer (suspicious for 32/64-bit architectures)
            PointerState::Invalid(addr)
        } else {
            PointerState::Valid(addr)
        };
    }

    /// Mark pointer read as failed
    pub fn mark_pointer_error(variable: &mut Variable) {
        if let Some(ptr_meta) = &mut variable.pointer_metadata {
            ptr_meta.pointer_state = PointerState::ReadError;
        }
    }

    /// Clear cached data (for reset/reconnect)
    pub fn clear(&mut self) {
        self.last_pointer_reads.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VariableType;

    fn create_test_variable(name: &str, address: u64, var_type: VariableType) -> Variable {
        Variable::new(name, address, var_type)
    }

    #[test]
    fn test_empty_variables() {
        let manager = ReadManager::new(64);
        let regions = manager.plan_reads(&[]);
        assert!(regions.is_empty());
    }

    #[test]
    fn test_single_variable() {
        let manager = ReadManager::new(64);
        let vars = vec![create_test_variable("var1", 0x2000_0000, VariableType::U32)];
        let regions = manager.plan_reads(&vars);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].address, 0x2000_0000);
        assert_eq!(regions[0].size, 4);
        assert_eq!(regions[0].variable_indices, vec![0]);
    }

    #[test]
    fn test_adjacent_variables() {
        let manager = ReadManager::new(64);
        let vars = vec![
            create_test_variable("var1", 0x2000_0000, VariableType::U32),
            create_test_variable("var2", 0x2000_0004, VariableType::U32),
            create_test_variable("var3", 0x2000_0008, VariableType::U32),
        ];
        let regions = manager.plan_reads(&vars);

        // Should be combined into one region
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].address, 0x2000_0000);
        assert_eq!(regions[0].size, 12); // 3 * 4 bytes
        assert_eq!(regions[0].variable_indices.len(), 3);
    }

    #[test]
    fn test_variables_with_gap() {
        let manager = ReadManager::new(64);
        let vars = vec![
            create_test_variable("var1", 0x2000_0000, VariableType::U32),
            create_test_variable("var2", 0x2000_0030, VariableType::U32), // 48 bytes gap, within threshold
        ];
        let regions = manager.plan_reads(&vars);

        // Should be combined (48 bytes gap < 64 threshold)
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].address, 0x2000_0000);
        assert_eq!(regions[0].size, 0x34); // 52 bytes (0x30 + 4)
    }

    #[test]
    fn test_variables_beyond_gap() {
        let manager = ReadManager::new(64);
        let vars = vec![
            create_test_variable("var1", 0x2000_0000, VariableType::U32),
            create_test_variable("var2", 0x2000_0100, VariableType::U32), // 256 bytes gap, beyond threshold
        ];
        let regions = manager.plan_reads(&vars);

        // Should be separate regions
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].address, 0x2000_0000);
        assert_eq!(regions[1].address, 0x2000_0100);
    }

    #[test]
    fn test_unsorted_variables() {
        let manager = ReadManager::new(64);
        let vars = vec![
            create_test_variable("var3", 0x2000_0008, VariableType::U32),
            create_test_variable("var1", 0x2000_0000, VariableType::U32),
            create_test_variable("var2", 0x2000_0004, VariableType::U32),
        ];
        let regions = manager.plan_reads(&vars);

        // Should still be combined (sorted internally)
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].address, 0x2000_0000);
        // Indices should map back to original positions
        assert!(regions[0].variable_indices.contains(&0));
        assert!(regions[0].variable_indices.contains(&1));
        assert!(regions[0].variable_indices.contains(&2));
    }

    #[test]
    fn test_extract_value() {
        let manager = ReadManager::new(64);
        let var = create_test_variable("test", 0x2000_0004, VariableType::U32);
        let region = ReadRegion {
            address: 0x2000_0000,
            size: 12,
            variable_indices: vec![0],
        };

        // Create test data: 12 bytes, with a U32 value of 0x12345678 at offset 4
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00,  // offset 0-3
            0x78, 0x56, 0x34, 0x12,  // offset 4-7 (little endian 0x12345678)
            0xFF, 0xFF, 0xFF, 0xFF,  // offset 8-11
        ];

        let value = manager.extract_value(&var, &region, &data);
        assert!(value.is_some());
        assert_eq!(value.unwrap(), 0x12345678 as f64);
    }

    #[test]
    fn test_extract_value_out_of_bounds() {
        let manager = ReadManager::new(64);
        let var = create_test_variable("test", 0x2000_1000, VariableType::U32);
        let region = ReadRegion {
            address: 0x2000_0000,
            size: 12,
            variable_indices: vec![0],
        };
        let data = vec![0u8; 12];

        // Variable is outside the region
        let value = manager.extract_value(&var, &region, &data);
        assert!(value.is_none());
    }

    #[test]
    fn test_calculate_savings() {
        let manager = ReadManager::new(64);

        // Adjacent variables should save reads
        let vars = vec![
            create_test_variable("var1", 0x2000_0000, VariableType::U32),
            create_test_variable("var2", 0x2000_0004, VariableType::U32),
            create_test_variable("var3", 0x2000_0008, VariableType::U32),
        ];
        let (bulk_reads, saved) = manager.calculate_savings(&vars);
        assert_eq!(bulk_reads, 1);
        assert_eq!(saved, 2); // Saved 2 individual reads

        // Distant variables won't save reads
        let vars2 = vec![
            create_test_variable("var1", 0x2000_0000, VariableType::U32),
            create_test_variable("var2", 0x2000_1000, VariableType::U32),
            create_test_variable("var3", 0x2000_2000, VariableType::U32),
        ];
        let (bulk_reads2, saved2) = manager.calculate_savings(&vars2);
        assert_eq!(bulk_reads2, 3);
        assert_eq!(saved2, 0); // No savings
    }

    #[test]
    fn test_mixed_variable_sizes() {
        let manager = ReadManager::new(64);
        let vars = vec![
            create_test_variable("var1", 0x2000_0000, VariableType::U8),
            create_test_variable("var2", 0x2000_0001, VariableType::U16),
            create_test_variable("var3", 0x2000_0004, VariableType::U32),
            create_test_variable("var4", 0x2000_0008, VariableType::F64),
        ];
        let regions = manager.plan_reads(&vars);

        // Should be combined
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].address, 0x2000_0000);
        // Total size: U8(1) + U16(2) at offset 1 (ends at 3) + U32(4) at offset 4 + F64(8) at offset 8 = 16 bytes
        assert_eq!(regions[0].size, 16);
    }
}
