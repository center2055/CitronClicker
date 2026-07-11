//! silent self-updater. checks github releases, stages a newer build for the next launch.
//! windows-only (in-place exe swap), no-op stub elsewhere.

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::*;
