//! os input layer. all win32 lives in `win` (cfg windows); `stub` mirrors the public surface as
//! no-ops so the rest of the app compiles everywhere.

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::*;
