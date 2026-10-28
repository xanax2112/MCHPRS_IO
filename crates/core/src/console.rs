//! Interactive terminal console (BUG-031).
//!
//! The server reads lines from stdin; anything that starts with `/` is handed
//! to the plot thread, which runs it as a command. Commands that need a player
//! target are rejected with a red message, everything else behaves like the
//! in-game command (including `/rtps 0`, which kicks the automation
//! connections).

use std::io::{BufRead, BufReader};

/// ANSI colours used by the console output.
const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";

/// Starts the stdin reader. `send` receives every `/` line verbatim.
pub fn spawn(send: impl Fn(String) + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("console".to_string())
        .spawn(move || {
            let stdin = std::io::stdin();
            for line in BufReader::new(stdin.lock()).lines() {
                let Ok(line) = line else {
                    break;
                };
                let line = line.trim();
                if !line.starts_with('/') {
                    continue;
                }
                send(line.to_string());
            }
        });
}

fn echo(colour: &str, text: &str) {
    println!("{colour}[console]{RESET} {text}");
}

/// A command succeeded (or produced information).
pub fn ok(text: &str) {
    echo(GREEN, text);
}

/// A command failed.
pub fn error(text: &str) {
    echo(RED, text);
}

/// A non-fatal note.
pub fn note(text: &str) {
    echo(YELLOW, text);
}

/// The message used when a command needs a player and cannot run from the
/// console.
pub const NEEDS_PLAYER: &str = "This command needs a player and cannot be used from the console";
