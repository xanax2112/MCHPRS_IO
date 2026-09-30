#![deny(rust_2018_idioms)]

#[macro_use]
mod utils;
mod config;
mod interaction;
mod permissions;
mod player;
pub mod plot;
pub mod server;
pub mod automation;

#[macro_use]
extern crate bitflags;
