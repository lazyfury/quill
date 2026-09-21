//! Small, dependency-free helpers shared across the `deepseek_balance` project.
//!
//! These are the concerns that are true of the *domain*, not of the HTTP layer or
//! the UI, and they are split by concern rather than piled into one file:
//!
//! - [`time`] owns every clock/stamp format: the UTC+8 wall-clock stamp, the
//!   `MM:SS` countdown and the RFC 3339 parser the reset stamps need.
//! - [`currency`] owns how an amount's code is rendered.
//!
//! Nothing here knows about HTTP, the UI or the platform, so it stays pure and
//! testable — and `deepseek_balance` can keep its network/UI modules focused.

pub mod currency;
pub mod time;
