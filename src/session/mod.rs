//! Session recording and playback module
//!
//! This module provides functionality for recording data collection sessions
//! and playing them back later for analysis. Sessions include all variable
//! data along with timing information and metadata.
//!
//! # Features
//!
//! - Record data collection sessions with full timing information
//! - Save sessions to disk for later analysis
//! - Play back sessions at original or variable speed
//! - Seek to specific times within a session
//! - Compare recorded sessions with live data

pub mod player;
pub mod recorder;
pub mod types;

pub use player::SessionPlayer;
pub use recorder::SessionRecorder;
pub use types::{SessionMetadata, SessionRecording, SessionState};
