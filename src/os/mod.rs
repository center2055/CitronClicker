//! OS input layer. All Win32 lives in `win` (cfg(windows)); `stub` mirrors the public
//! surface as no-ops so the rest of the app compiles on every platform.

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::*;
