//! The parts of Zervo that do not know the engine exists.
//!
//! Everything here is arithmetic, colour, bytes or files: the theme and the
//! glass material it builds, the card grid, the trackpad gesture recogniser,
//! the small HTTPS client the wallpaper fetcher rides on, and the atomic JSON
//! store the rest of the browser keeps its settings in.
//!
//! It is a separate crate for one reason, and it is not architectural purity.
//! Compiling the binary means compiling Servo, which is the better part of an
//! hour, so `cargo clippy` and `cargo test` could not run on a pull request and
//! never did — the tests in here had never been executed by CI at all. This
//! crate builds in seconds against nothing heavier than egui and rustls, so
//! both can, and do.
//!
//! The binary re-exports every module below at its own root, so a call site
//! still writes `crate::theme::Palette` and nothing had to move for this.

pub mod gestures;
pub mod glass;
pub mod grid;
pub mod net;
pub mod store;
pub mod theme;
