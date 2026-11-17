//! The computer around the game: Steam and the EA app, running processes, drives, and the Windows
//! settings and policies that decide whether a program may start.

pub mod app_control;
pub mod disk;
pub mod ea_app;
pub mod process;
pub mod run_as_admin;
pub mod steam;
pub mod steam_client;
pub mod vdf;
