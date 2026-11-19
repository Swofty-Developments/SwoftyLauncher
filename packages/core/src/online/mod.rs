//! What the launcher does online: calls to its own server, the signed server modules it loads at
//! runtime, Time Trial boards, and online sessions between launchers.

pub mod modules;
pub mod multiplayer;
pub mod server;
pub mod timetrial;

#[cfg(test)]
mod contract;
