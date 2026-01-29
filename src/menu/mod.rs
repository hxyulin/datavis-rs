//! Native menu bar module using muda
//!
//! Provides platform-native menus for macOS, Windows, and Linux.

mod builder;
mod events;
mod ids;

pub use builder::{build_menu_bar, MenuBarState};
pub use events::MenuEvent;
pub use ids::MenuId;
