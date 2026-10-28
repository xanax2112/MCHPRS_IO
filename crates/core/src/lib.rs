#![deny(rust_2018_idioms)]

#[macro_use]
mod utils;
mod config;
pub mod console;
mod interaction;
mod io;
mod memory;
mod permissions;
mod player;
pub mod plot;
pub mod server;
pub mod automation;

#[macro_use]
extern crate bitflags;
