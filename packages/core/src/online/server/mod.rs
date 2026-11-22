//! Calls to the launcher's own server: sign-in, support and usage reports, the Time Trial boards
//! API and the integrity module it publishes.

pub mod api;
pub mod http;
pub mod integrity;
pub mod sign_in;
pub mod support;
pub mod usage;
