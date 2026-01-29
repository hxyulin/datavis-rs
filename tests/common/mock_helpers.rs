//! Mock construction helpers

use crossbeam_channel::{bounded, Receiver, Sender};

#[cfg(feature = "mock-probe")]
use datavis_rs::backend::{MockDataPattern, MockProbeBackend};

/// Create test channels with default size
pub fn create_test_channels<T, U>() -> (Sender<T>, Receiver<T>, Sender<U>, Receiver<U>) {
    let (tx1, rx1) = bounded(16);
    let (tx2, rx2) = bounded(16);
    (tx1, rx1, tx2, rx2)
}

#[cfg(feature = "mock-probe")]
pub fn create_test_mock_probe() -> MockProbeBackend {
    MockProbeBackend::new().with_default_pattern(MockDataPattern::Sine {
        frequency: 1.0,
        amplitude: 100.0,
        offset: 0.0,
    })
}
