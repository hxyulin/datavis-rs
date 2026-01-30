//! Zero-allocation data packet for the pipeline hot path.
//!
//! `DataPacket` is a fixed-size inline buffer of `Sample` values.
//! It is allocated once per node slot and reused every tick — no heap
//! allocation occurs during normal pipeline operation.

use crate::pipeline::id::VarId;
use std::time::Duration;

/// Maximum number of variable samples per packet.
/// 256 samples * 20 bytes = ~5KB — fits comfortably in L1 cache.
pub const MAX_PACKET_VARS: usize = 256;

/// A single variable sample within a `DataPacket`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Sample {
    /// Which variable this sample belongs to.
    pub var_id: VarId,
    /// Raw value read from the target.
    pub raw: f64,
    /// Converted value (after script transform, or same as raw).
    pub converted: f64,
}

impl Default for Sample {
    fn default() -> Self {
        Self {
            var_id: VarId::INVALID,
            raw: 0.0,
            converted: 0.0,
        }
    }
}

impl std::fmt::Debug for Sample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sample")
            .field("var_id", &self.var_id)
            .field("raw", &self.raw)
            .field("converted", &self.converted)
            .finish()
    }
}

/// Fixed-size inline data packet — the primary data transfer unit in the pipeline.
///
/// Never heap-allocated during tick processing. Each `NodeSlot` owns two of these
/// (input + output) that are reused every tick.
pub struct DataPacket {
    pub timestamp: Duration,
    samples: [Sample; MAX_PACKET_VARS],
    len: u16,
}

impl DataPacket {
    /// Create a new empty packet.
    pub fn new() -> Self {
        Self {
            timestamp: Duration::ZERO,
            // Safety: Sample is Copy + repr(C) with no padding concerns.
            // Default-initializing to zeros is valid.
            samples: [Sample::default(); MAX_PACKET_VARS],
            len: 0,
        }
    }

    /// Clear the packet for reuse (just resets length — no zeroing needed).
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
        self.timestamp = Duration::ZERO;
    }

    /// Number of samples currently in the packet.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether the packet is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether the packet is full.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len as usize >= MAX_PACKET_VARS
    }

    /// Push a sample. Returns `false` if full.
    #[inline]
    pub fn push(&mut self, sample: Sample) -> bool {
        if self.is_full() {
            return false;
        }
        self.samples[self.len as usize] = sample;
        self.len += 1;
        true
    }

    /// Push a sample from components. Returns `false` if full.
    #[inline]
    pub fn push_value(&mut self, var_id: VarId, raw: f64, converted: f64) -> bool {
        self.push(Sample {
            var_id,
            raw,
            converted,
        })
    }

    /// Get sample at index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Sample> {
        if index < self.len as usize {
            Some(&self.samples[index])
        } else {
            None
        }
    }

    /// Iterate over valid samples.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Sample> {
        self.samples[..self.len as usize].iter()
    }

    /// Iterate mutably over valid samples.
    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Sample> {
        self.samples[..self.len as usize].iter_mut()
    }

    /// Get the samples as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[Sample] {
        &self.samples[..self.len as usize]
    }

    /// Copy all samples from `src` into `self`, replacing existing content.
    #[inline]
    pub fn copy_from(&mut self, src: &DataPacket) {
        let n = src.len as usize;
        self.samples[..n].copy_from_slice(&src.samples[..n]);
        self.len = src.len;
        self.timestamp = src.timestamp;
    }

    /// Append all samples from `src` to `self`. Drops samples that don't fit.
    pub fn append_from(&mut self, src: &DataPacket) {
        let remaining = MAX_PACKET_VARS - self.len as usize;
        let to_copy = (src.len as usize).min(remaining);
        if to_copy > 0 {
            let start = self.len as usize;
            self.samples[start..start + to_copy].copy_from_slice(&src.samples[..to_copy]);
            self.len += to_copy as u16;
        }
    }
}

impl Default for DataPacket {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DataPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataPacket")
            .field("timestamp", &self.timestamp)
            .field("len", &self.len)
            .finish()
    }
}

/// Events that can flow between nodes alongside data.
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    /// Collection started.
    Started,
    /// Collection stopped.
    Stopped,
    /// A variable was added to the tree.
    VariableAdded(VarId),
    /// A variable was removed from the tree.
    VariableRemoved(VarId),
    /// Variable enable/disable changed.
    VariableToggled(VarId),
    /// Error on a specific variable.
    VariableError { var_id: VarId, message: String },
    /// General node error.
    Error(String),
}

/// Configuration values that can be sent to nodes.
#[derive(Debug, Clone)]
pub enum ConfigValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl ConfigValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            ConfigValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            ConfigValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            ConfigValue::String(v) => Some(v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_push_and_iter() {
        let mut pkt = DataPacket::new();
        assert!(pkt.is_empty());

        pkt.push_value(VarId(0), 1.0, 1.0);
        pkt.push_value(VarId(1), 2.0, 4.0);
        assert_eq!(pkt.len(), 2);

        let samples: Vec<_> = pkt.iter().collect();
        assert_eq!(samples[0].var_id, VarId(0));
        assert_eq!(samples[0].raw, 1.0);
        assert_eq!(samples[1].var_id, VarId(1));
        assert_eq!(samples[1].converted, 4.0);
    }

    #[test]
    fn test_packet_clear() {
        let mut pkt = DataPacket::new();
        pkt.push_value(VarId(0), 1.0, 1.0);
        assert!(!pkt.is_empty());

        pkt.clear();
        assert!(pkt.is_empty());
        assert_eq!(pkt.len(), 0);
    }

    #[test]
    fn test_packet_full() {
        let mut pkt = DataPacket::new();
        for i in 0..MAX_PACKET_VARS {
            assert!(pkt.push_value(VarId(i as u32), i as f64, i as f64));
        }
        assert!(pkt.is_full());
        assert!(!pkt.push_value(VarId(9999), 0.0, 0.0));
    }

    #[test]
    fn test_packet_copy_from() {
        let mut src = DataPacket::new();
        src.timestamp = Duration::from_millis(100);
        src.push_value(VarId(0), 1.0, 2.0);
        src.push_value(VarId(1), 3.0, 4.0);

        let mut dst = DataPacket::new();
        dst.copy_from(&src);
        assert_eq!(dst.len(), 2);
        assert_eq!(dst.timestamp, Duration::from_millis(100));
        assert_eq!(dst.get(0).unwrap().raw, 1.0);
    }

    #[test]
    fn test_packet_append_from() {
        let mut a = DataPacket::new();
        a.push_value(VarId(0), 1.0, 1.0);

        let mut b = DataPacket::new();
        b.push_value(VarId(1), 2.0, 2.0);
        b.push_value(VarId(2), 3.0, 3.0);

        a.append_from(&b);
        assert_eq!(a.len(), 3);
    }

    #[test]
    fn test_sample_size() {
        // Sample is 24 bytes: VarId(u32)=4 + 4 padding + f64=8 + f64=8
        // due to f64 alignment requirements. Still fits well in L1 cache:
        // 256 samples * 24 bytes = 6KB.
        assert_eq!(std::mem::size_of::<Sample>(), 24);
    }

    #[test]
    #[allow(clippy::approx_constant)] // Intentionally using 3.14 as test value, not PI
    fn test_config_value() {
        assert_eq!(ConfigValue::Bool(true).as_bool(), Some(true));
        assert_eq!(ConfigValue::Int(42).as_int(), Some(42));
        assert_eq!(ConfigValue::Float(3.14).as_float(), Some(3.14));
        assert_eq!(ConfigValue::String("hello".into()).as_str(), Some("hello"));
    }
}
