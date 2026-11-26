//! Limakhos self-hosted multiplayer server library.

pub mod auth;
pub mod db;
pub mod glicko2;
pub mod room;
pub mod routes;

pub use routes::{build_router, AppState};
