//! silent self-updater. checks github releases and stages a newer build to take effect on the
//! next launch. windows-only (the in-place exe swap); a no-op stub elsewhere.

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::*;
