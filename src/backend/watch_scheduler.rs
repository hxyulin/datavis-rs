//! Live Watch scheduler.
//!
//! Decoupled from the high-rate variable poll loop: ticks at a configurable
//! low rate (default 5 Hz) whenever the probe is connected, regardless of
//! whether `StartCollection` is active. Reads only the leaves the frontend
//! has marked as visible-and-expanded; pointer leaves' values are cached so
//! children with `WatchAddress::PointerDeref` can resolve their addresses on
//! the *next* tick.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::backend::probe_trait::DebugProbe;
use crate::types::PointerRuntime;
use crate::watch::{WatchAddress, WatchId, WatchLeafRead, WatchValue};

/// One-tick lag for pointer-deref resolution is acceptable — same as the
/// existing `PointerRuntime` behaviour for plotted variables.
pub struct WatchScheduler {
    /// Active leaves keyed by `(root_id, path)`. Replaced wholesale via
    /// `set_leaves`.
    leaves: HashMap<(WatchId, String), WatchLeafRead>,
    /// Last cached pointer value per `(root_id, parent_path)` — used to resolve
    /// `WatchAddress::PointerDeref` for children on subsequent ticks.
    pointer_cache: HashMap<(WatchId, String), PointerRuntime>,
    poll_rate_hz: u32,
    /// `None` until the first tick has been performed.
    last_tick: Option<Instant>,
}

impl WatchScheduler {
    pub fn new(poll_rate_hz: u32) -> Self {
        Self {
            leaves: HashMap::new(),
            pointer_cache: HashMap::new(),
            poll_rate_hz,
            last_tick: None,
        }
    }

    pub fn set_leaves(&mut self, leaves: Vec<WatchLeafRead>) {
        // Drop pointer-cache entries that no longer correspond to any leaf,
        // otherwise stale pointer values could resolve addresses that were
        // collapsed but never overwritten.
        let new: HashMap<_, _> = leaves
            .into_iter()
            .map(|l| ((l.root_id, l.path.clone()), l))
            .collect();
        self.pointer_cache.retain(|key, _| new.contains_key(key));
        self.leaves = new;
    }

    pub fn set_poll_rate(&mut self, hz: u32) {
        self.poll_rate_hz = hz;
        // Force the next tick to fire immediately at the new rate.
        self.last_tick = None;
    }

    pub fn clear(&mut self) {
        self.leaves.clear();
        self.pointer_cache.clear();
    }

    /// Returns true if it's time to perform a watch read this iteration.
    pub fn should_tick(&self) -> bool {
        if self.poll_rate_hz == 0 || self.leaves.is_empty() {
            return false;
        }
        match self.last_tick {
            None => true,
            Some(t) => {
                let interval = Duration::from_secs_f64(1.0 / self.poll_rate_hz as f64);
                t.elapsed() >= interval
            }
        }
    }

    /// Perform one read pass. Caller must ensure the probe is connected.
    /// Returns the (root_id, path, WatchValue) tuples to publish to the UI.
    pub fn tick(&mut self, probe: &mut dyn DebugProbe) -> Vec<(WatchId, String, WatchValue)> {
        self.last_tick = Some(Instant::now());

        // Snapshot the leaf list — we'll iterate without holding a borrow on
        // `self` so we can update pointer_cache as we go.
        let leaves: Vec<WatchLeafRead> = self.leaves.values().cloned().collect();

        let mut out = Vec::with_capacity(leaves.len());
        for leaf in leaves {
            let result = self.read_one(probe, &leaf);
            // Update pointer cache on success so dependent children can resolve
            // on the next tick.
            if leaf.is_pointer {
                let entry = self
                    .pointer_cache
                    .entry((leaf.root_id, leaf.path.clone()))
                    .or_default();
                match &result.raw {
                    Ok(v) => entry.update_from_read(*v),
                    Err(_) => entry.mark_error(),
                }
            }
            out.push((leaf.root_id, leaf.path.clone(), result));
        }
        out
    }

    fn read_one(&self, probe: &mut dyn DebugProbe, leaf: &WatchLeafRead) -> WatchValue {
        let address = match self.resolve_address(leaf) {
            Some(addr) => addr,
            None => {
                return WatchValue {
                    raw: Err("pointer not yet resolved".to_string()),
                    pointer_state: if leaf.is_pointer {
                        Some(crate::types::PointerState::Unread)
                    } else {
                        None
                    },
                };
            }
        };

        let size = leaf.var_type.size_bytes();
        if size == 0 {
            return WatchValue {
                raw: Err("zero-size type".to_string()),
                pointer_state: None,
            };
        }

        let bytes = match probe.read_memory(address, size) {
            Ok(b) => b,
            Err(e) => {
                return WatchValue {
                    raw: Err(e.to_string()),
                    pointer_state: if leaf.is_pointer {
                        Some(crate::types::PointerState::ReadError)
                    } else {
                        None
                    },
                };
            }
        };

        let raw = match leaf.var_type.parse_to_f64(&bytes) {
            Some(v) => v,
            None => {
                return WatchValue {
                    raw: Err("short read".to_string()),
                    pointer_state: None,
                };
            }
        };

        let pointer_state = if leaf.is_pointer {
            // Reuse PointerRuntime classification logic.
            let mut rt = PointerRuntime::default();
            rt.update_from_read(raw);
            Some(rt.pointer_state)
        } else {
            None
        };

        WatchValue {
            raw: Ok(raw),
            pointer_state,
        }
    }

    fn resolve_address(&self, leaf: &WatchLeafRead) -> Option<u64> {
        match &leaf.address {
            WatchAddress::Static(addr) => Some(*addr),
            WatchAddress::PointerDeref {
                parent_path,
                offset,
            } => self
                .pointer_cache
                .get(&(leaf.root_id, parent_path.clone()))
                .and_then(|rt| rt.resolve_address(*offset)),
        }
    }

    #[cfg(test)]
    pub fn leaves_len(&self) -> usize {
        self.leaves.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VariableType;
    use crate::watch::WatchAddress;

    fn leaf(root: u32, path: &str, addr: u64, t: VariableType, is_ptr: bool) -> WatchLeafRead {
        WatchLeafRead {
            root_id: WatchId(root),
            path: path.to_string(),
            address: WatchAddress::Static(addr),
            var_type: t,
            is_pointer: is_ptr,
        }
    }

    #[test]
    fn test_set_leaves_replaces_set() {
        let mut s = WatchScheduler::new(5);
        s.set_leaves(vec![leaf(1, "", 0x1000, VariableType::U32, false)]);
        assert_eq!(s.leaves_len(), 1);
        s.set_leaves(vec![]);
        assert_eq!(s.leaves_len(), 0);
    }

    #[test]
    fn test_set_leaves_drops_stale_pointer_cache() {
        let mut s = WatchScheduler::new(5);
        // Seed pointer cache for a leaf that's about to be removed.
        s.pointer_cache.insert(
            (WatchId(1), "".to_string()),
            crate::types::PointerRuntime::default(),
        );
        s.set_leaves(vec![leaf(2, "", 0x1000, VariableType::U32, false)]);
        assert!(!s.pointer_cache.contains_key(&(WatchId(1), "".to_string())));
    }

    #[test]
    fn test_should_tick_disabled_when_rate_zero() {
        let mut s = WatchScheduler::new(0);
        s.set_leaves(vec![leaf(1, "", 0x1000, VariableType::U32, false)]);
        assert!(!s.should_tick());
    }

    #[test]
    fn test_should_tick_disabled_when_no_leaves() {
        let s = WatchScheduler::new(10);
        assert!(!s.should_tick());
    }

    #[test]
    fn test_should_tick_after_interval() {
        let mut s = WatchScheduler::new(1000);
        s.set_leaves(vec![leaf(1, "", 0x1000, VariableType::U32, false)]);
        // We initialised last_tick far in the past so this should fire immediately.
        assert!(s.should_tick());
    }
}
