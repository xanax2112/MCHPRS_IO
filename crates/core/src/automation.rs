//! Automation bridge: a plain-text TCP protocol (default port 25585, bound to
//! loopback only) that lets external scripts drive and observe the IO groups
//! declared in `io.txt`.
//!
//! This is the 0.5.2-alpha wire format; the contract this module implements
//! (BACKLOG.md §3) is:
//!
//! ```text
//! HELLO MCHPRS_IO 0.5.2-alpha <tick>        (compiled)
//! HELLO MCHPRS_IO 0.5.2-alpha               (not compiled: no tick field)
//!
//! INPUT <name> <hex>            -> OK INPUT <name> <absolute tick>
//! INPUT <name> <offset> <hex>   -> OK INPUT <name> <absolute tick>   [seq mode only]
//!
//! POUT <name>                   -> OUTPUT <name> <tick> <hex>        (bind + current value)
//! OUTPUT <name> <tick> <hex>                                         (push)
//!
//! READ <name>:<bit>             -> OK READ <name>:<bit> <tick> <power>
//! READ <x,y,z>                  -> OK READ <x,y,z> <tick> <power>
//!
//! TICK NOW                      -> NOW <tick>
//!
//! RADV <x>                      -> OK RADV <x> <tick>                [rtps = 0 only]
//!
//! Errors are a single `ERR <code>` line, with a lower-case code. A line never
//! produces more than one error.
//! ```
//!
//! Every verb must be written in upper case; `input`, `radv` or `tick now`
//! answer `ERR unknown` (BACKLOG.md BUG-007).
//!
//! Scripts have **no** `/` command channel: any line beginning with `/` is
//! answered with `ERR unknown` (BACKLOG.md BUG-005). `PIN` / `SUB` / `UNSUB` /
//! `EVENT` / `USE` / `SET` / `TICK STEP` and the old data-port coordinate
//! syntax were removed as well; they answer `ERR unknown` too.
//!
//! The bridge is a thin protocol layer: it owns the sockets, parses the wire
//! format and keeps the per-connection state that the protocol needs, but
//! everything that touches a compiled circuit (groups, `READ` values, tick
//! counter, reservations) is answered by the plot thread, which owns the
//! `IoTable` and the [`mchprs_redpiler::Compiler`].

use crate::memory::{right_align, MatrixShape};
use mchprs_blocks::BlockPos;
use rustc_hash::FxHashMap;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use tracing::{debug, info, warn};

/// The protocol version announced in `HELLO`.
pub const PROTOCOL_VERSION: &str = "0.5.4-alpha";

/// Maximum length of a single request or response line (BACKLOG.md §7.11).
const MAX_LINE_LEN: usize = 65_536;

/// Maximum amount of bytes queued for a connection that is not reading. A
/// client that lets more than this pile up is dropped instead of letting the
/// queue grow without bound (BACKLOG.md §7.11).
const MAX_PENDING: usize = 1024 * 1024;

/// Group-name based requests carry no coordinates. They are routed to the one
/// plot this server ever has (BACKLOG.md BUG-010); `/` commands use the same
/// model.
const ROUTE_ANCHOR: BlockPos = BlockPos::new(0, 64, 0);

/// What an automation client asked for.
///
/// The `exclusive` flag records whether the connection that sent the request
/// is the one holding the `rtps 0` lock (BACKLOG.md §7.13); reservations are
/// only accepted from that connection.
#[derive(Debug, Clone)]
pub struct AutomationRequest {
    /// The sending connection holds the `rtps 0` exclusive lock.
    pub exclusive: bool,
    pub kind: RequestKind,
}

/// The payload of an [`AutomationRequest`].
#[derive(Debug, Clone)]
pub enum RequestKind {
    /// Handshake: the plot answers with its compile state and tick counter.
    Hello,
    /// `POUT <name>`: bind this connection to the output group `name`.
    Bind { name: String },
    /// `INPUT ...`: drive an input group, optionally at a reserved tick.
    Input {
        name: String,
        /// `Some` for the `INPUT <name> <offset> <hex>` form.
        offset: Option<u16>,
        hex: String,
    },
    /// `READ <name>:<bit>`: read one point of a group.
    ReadBit { name: String, bit: u32 },
    /// `READ <x,y,z>`: read the IO element at an absolute coordinate.
    ReadPos { pos: BlockPos },
    /// `TICK NOW`: report the latest completed tick.
    TickNow,
    /// `RADV <ticks>`: advance the circuit manually (BACKLOG.md BUG-008).
    Radv { ticks: u32 },
}

impl AutomationRequest {
    /// The plot a request has to be routed to.
    pub fn pos(&self) -> BlockPos {
        match &self.kind {
            RequestKind::ReadPos { pos } => *pos,
            _ => ROUTE_ANCHOR,
        }
    }
}

/// The reply a plot thread sends back after handling an [`AutomationRequest`].
#[derive(Debug, Clone)]
pub enum AutomationResponse {
    /// Handshake reply. `tick` is `None` while no circuit is compiled.
    Hello { tick: Option<u32> },
    /// `OUTPUT <name> <tick> <hex>`: the current value of a group, sent on
    /// `POUT` and whenever the group fires.
    Output { name: String, tick: u32, hex: String },
    /// `OK INPUT <name> <tick>`.
    Input { name: String, tick: u32 },
    /// `OK READ <target> <tick> <power>`.
    Read { target: String, tick: u32, power: u8 },
    /// `NOW <tick>`.
    Now { tick: u32 },
    /// `OK RADV <ticks> <tick>`.
    Radv { ticks: u32, tick: u32 },
    /// `ERR <code>`; the code comes from the table in BACKLOG.md §3.7 and is
    /// always lower case.
    Err(&'static str),
    /// The request reached no running circuit; answered with `ERR nocompile`
    /// (or a tick-less `HELLO` while the handshake is still pending).
    Inactive(BlockPos),
    /// The request was handled and needs no wire reply (for example a repeated
    /// `POUT`, which is idempotent).
    CommandOk,
}

/// An action the server thread must perform on behalf of the automation
/// bridge.
#[derive(Debug)]
pub enum AutomationAction {
    /// Route an automation request to the plot containing `pos`.
    Route {
        conn_id: u64,
        pos: BlockPos,
        request: AutomationRequest,
    },
    /// An automation connection closed; the plot holding its `POUT` bindings
    /// must drop them so they stop being sampled and pushed.
    Disconnect {
        conn_id: u64,
        plot_x: i32,
        plot_z: i32,
    },
}

struct AutomationConn {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    /// Bytes read so far without a terminating newline.
    partial: Vec<u8>,
    /// Bytes queued to be written to the socket.
    pending: Vec<u8>,
    /// The connection is waiting for the plot's handshake reply.
    hello_pending: bool,
    /// Close the connection as soon as `pending` has been written out.
    closing: bool,
}

/// Manages the automation TCP listener and all its connections.
pub struct AutomationServer {
    listener: TcpListener,
    next_conn_id: u64,
    connections: FxHashMap<u64, AutomationConn>,
    /// `/tick seq` just ran: drop everyone and let the next connection become
    /// the single exclusive script (BACKLOG.md BUG-004).
    waiting: bool,
    /// The one script connection allowed while reservation mode is active.
    exclusive: Option<u64>,
    /// Connections dropped since the last `poll`, so the plots can forget them.
    pending_disconnects: Vec<(u64, i32, i32)>,
}

impl AutomationServer {
    /// Binds the automation listener to `127.0.0.1:port`. Loopback only: the
    /// automation bridge is meant for local scripts, not remote clients.
    pub fn bind(port: i64) -> std::io::Result<Self> {
        let port = match u16::try_from(port) {
            Ok(port) => port,
            Err(_) => {
                warn!(
                    "io_port {} is not a valid port number, falling back to 25585",
                    port
                );
                25585
            }
        };
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        Ok(AutomationServer {
            listener,
            next_conn_id: 0,
            connections: FxHashMap::default(),
            waiting: false,
            exclusive: None,
            pending_disconnects: Vec::new(),
        })
    }

    /// Accepts new connections, reads and parses every pending request line,
    /// and returns the actions that need to be routed to a plot.
    ///
    /// Everything that can be answered locally (protocol errors, line length)
    /// is written to the connection's output queue directly and picked up by
    /// [`AutomationServer::flush`].
    pub fn poll(&mut self) -> Vec<AutomationAction> {
        let mut actions = Vec::new();
        self.accept_new(&mut actions);

        let mut replies: Vec<(u64, String)> = Vec::new();
        let mut close_now: Vec<u64> = Vec::new();
        let mut close_after_flush: Vec<u64> = Vec::new();
        let mut routed: Vec<AutomationAction> = Vec::new();

        let conn_ids: Vec<u64> = self.connections.keys().copied().collect();
        for conn_id in conn_ids {
            let exclusive = self.exclusive == Some(conn_id);
            let mut raw_lines: Vec<Vec<u8>> = Vec::new();
            let outcome = match self.connections.get_mut(&conn_id) {
                Some(conn) => read_lines(conn, &mut raw_lines),
                None => continue,
            };
            match outcome {
                // A closed or broken socket is dropped right away; there is
                // nobody left to read an error line.
                ReadOutcome::Eof | ReadOutcome::Io => {
                    close_now.push(conn_id);
                    continue;
                }
                // The reply has to reach the client, so the socket stays open
                // until `flush` has written it.
                ReadOutcome::LineTooLong => {
                    replies.push((conn_id, "ERR line\n".to_string()));
                    close_after_flush.push(conn_id);
                    continue;
                }
                ReadOutcome::Ok => {}
            }
            for raw in raw_lines {
                let line = match std::str::from_utf8(&raw) {
                    Ok(line) => line.trim(),
                    Err(_) => {
                        // Never silently repair a broken payload (BACKLOG.md
                        // §7.11); answer and move on with the next line.
                        replies.push((conn_id, "ERR encoding\n".to_string()));
                        continue;
                    }
                };
                if line.is_empty() {
                    continue;
                }
                match parse_request(line, exclusive) {
                    Parsed::Request(request) => routed.push(AutomationAction::Route {
                        conn_id,
                        pos: request.pos(),
                        request,
                    }),
                    Parsed::Error(code) => {
                        replies.push((conn_id, format!("ERR {}\n", code)));
                    }
                }
            }
        }

        for (conn_id, line) in replies {
            self.queue_line(conn_id, line);
        }
        for conn_id in close_after_flush {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.closing = true;
            }
        }
        for conn_id in close_now {
            self.drop_connection(conn_id);
        }

        actions.extend(routed);
        actions.extend(
            self.pending_disconnects
                .drain(..)
                .map(|(conn_id, plot_x, plot_z)| AutomationAction::Disconnect {
                    conn_id,
                    plot_x,
                    plot_z,
                }),
        );
        actions
    }

    /// Flushes queued responses to all connections, removing dead sockets and
    /// connections that were marked for closing.
    pub fn flush(&mut self) {
        let mut closed: Vec<u64> = Vec::new();
        let conn_ids: Vec<u64> = self.connections.keys().copied().collect();
        for conn_id in conn_ids {
            let finished = {
                let Some(conn) = self.connections.get_mut(&conn_id) else {
                    continue;
                };
                loop {
                    if conn.pending.is_empty() {
                        break;
                    }
                    match conn.writer.write(&conn.pending) {
                        Ok(0) => {
                            closed.push(conn_id);
                            break;
                        }
                        Ok(written) => {
                            conn.pending.drain(..written);
                        }
                        Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => {
                            closed.push(conn_id);
                            break;
                        }
                    }
                }
                conn.closing && conn.pending.is_empty()
            };
            if finished {
                closed.push(conn_id);
            }
        }
        for conn_id in closed {
            self.drop_connection(conn_id);
        }
    }

    /// Handles a reply from a plot thread for a previously routed request.
    pub fn handle_reply(&mut self, conn_id: u64, reply: AutomationResponse) {
        let hello_pending = self
            .connections
            .get(&conn_id)
            .is_some_and(|conn| conn.hello_pending);
        let line = match reply {
            AutomationResponse::Hello { tick } => {
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.hello_pending = false;
                }
                match tick {
                    Some(tick) => format!("HELLO MCHPRS_IO {} {}\n", PROTOCOL_VERSION, tick),
                    None => format!("HELLO MCHPRS_IO {}\n", PROTOCOL_VERSION),
                }
            }
            AutomationResponse::Output { name, tick, hex } => {
                format!("OUTPUT {} {} {}\n", name, tick, hex)
            }
            AutomationResponse::Input { name, tick } => {
                format!("OK INPUT {} {}\n", name, tick)
            }
            AutomationResponse::Read {
                target,
                tick,
                power,
            } => format!("OK READ {} {} {}\n", target, tick, power),
            AutomationResponse::Now { tick } => format!("NOW {}\n", tick),
            AutomationResponse::Radv { ticks, tick } => {
                format!("OK RADV {} {}\n", ticks, tick)
            }
            AutomationResponse::Err(code) => format!("ERR {}\n", code),
            // No plot runs the requested circuit. If the handshake is still
            // pending this is its answer: an uncompiled server omits the tick
            // field (BACKLOG.md §7.1).
            AutomationResponse::Inactive(_) => {
                if hello_pending {
                    if let Some(conn) = self.connections.get_mut(&conn_id) {
                        conn.hello_pending = false;
                    }
                    format!("HELLO MCHPRS_IO {}\n", PROTOCOL_VERSION)
                } else {
                    "ERR nocompile\n".to_string()
                }
            }
            AutomationResponse::CommandOk => return,
        };
        self.queue_line(conn_id, line);
    }

    /// Pushes a sampled `OUTPUT` line to a connection bound to that group.
    pub fn push_output(&mut self, conn_id: u64, name: &str, tick: u32, hex: &str) {
        self.queue_line(conn_id, format!("OUTPUT {} {} {}\n", name, tick, hex));
    }

    /// Player ran `/tick seq` (`on`) or left reservation mode with `/rtps > 0`
    /// (`off`, BACKLOG.md BUG-004 / BUG-005).
    ///
    /// Turning the mode on always drops every connection, so the next one to
    /// arrive becomes the single exclusive script; running it again re-arms
    /// the session instead of being a no-op. Turning it off keeps the current
    /// connections but unlocks reservations.
    pub fn set_seq_mode(&mut self, on: bool) {
        if on {
            let conn_ids: Vec<u64> = self.connections.keys().copied().collect();
            info!(
                "tick seq: dropping {} automation connection(s); the next one to connect is the exclusive script",
                conn_ids.len()
            );
            for conn_id in conn_ids {
                self.drop_connection(conn_id);
            }
            self.exclusive = None;
            self.waiting = true;
        } else {
            self.waiting = false;
            self.exclusive = None;
        }
    }

    fn accept_new(&mut self, actions: &mut Vec<AutomationAction>) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    // While an exclusive script holds the plot, everyone else
                    // is refused outright (BACKLOG.md BUG-004).
                    if self.exclusive.is_some() {
                        info!(
                            "Refusing automation client {}: a /tick seq exclusive session is active",
                            addr
                        );
                        drop(stream);
                        continue;
                    }
                    if let Err(err) = stream.set_nonblocking(true) {
                        warn!("Failed to set automation socket non-blocking: {}", err);
                        continue;
                    }
                    let reader = match stream.try_clone() {
                        Ok(stream) => BufReader::new(stream),
                        Err(err) => {
                            warn!("Failed to clone automation socket for {}: {}", addr, err);
                            continue;
                        }
                    };
                    debug!("Automation client connected from {}", addr);
                    let conn_id = self.next_conn_id;
                    self.next_conn_id = self.next_conn_id.wrapping_add(1);
                    if self.waiting {
                        self.waiting = false;
                        self.exclusive = Some(conn_id);
                        info!(
                            "Automation connection {} became the /tick seq exclusive script",
                            conn_id
                        );
                    }
                    self.connections.insert(
                        conn_id,
                        AutomationConn {
                            writer: stream,
                            reader,
                            partial: Vec::new(),
                            pending: Vec::new(),
                            hello_pending: true,
                            closing: false,
                        },
                    );
                    // The handshake carries the plot's tick counter, so it is
                    // answered by the plot rather than here.
                    actions.push(AutomationAction::Route {
                        conn_id,
                        pos: ROUTE_ANCHOR,
                        request: AutomationRequest {
                            exclusive: self.exclusive == Some(conn_id),
                            kind: RequestKind::Hello,
                        },
                    });
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => return,
                Err(err) => {
                    warn!("Automation listener accept failed: {}", err);
                    return;
                }
            }
        }
    }

    fn queue_line(&mut self, conn_id: u64, line: String) {
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return;
        };
        if conn.closing {
            return;
        }
        conn.pending.extend_from_slice(line.as_bytes());
        if conn.pending.len() > MAX_PENDING {
            // The client stopped reading; drop it instead of buffering forever.
            warn!(
                "Automation connection {} fell more than {} bytes behind, dropping it",
                conn_id, MAX_PENDING
            );
            conn.pending.clear();
            conn.closing = true;
        }
    }

    fn drop_connection(&mut self, conn_id: u64) {
        if self.connections.remove(&conn_id).is_none() {
            return;
        }
        debug!("Automation connection {} closed", conn_id);
        if self.exclusive == Some(conn_id) {
            // The only writer is gone: unlock, but do not hand the session to
            // the next connection; the player has to run `/tick seq` again
            // (BACKLOG.md BUG-004).
            self.exclusive = None;
            self.waiting = false;
        }
        self.pending_disconnects.push((conn_id, 0, 0));
    }
}

/// A request line parsed by [`parse_request`].
enum Parsed {
    /// A request that has to be routed to a plot.
    Request(AutomationRequest),
    /// A protocol error; exactly one `ERR` line is sent for it.
    Error(&'static str),
}

/// Parses one client request line. Every failure maps to a single lower-case
/// error code (BACKLOG.md §3.7); a line never reports more than one error.
fn parse_request(line: &str, exclusive: bool) -> Parsed {
    // Scripts have no command channel: anything that looks like a `/` command
    // is refused outright (BACKLOG.md BUG-008).
    if line.starts_with('/') {
        return Parsed::Error("unknown");
    }

    let mut parts = line.split_whitespace();
    let Some(raw_verb) = parts.next() else {
        return Parsed::Error("unknown");
    };
    let request = |kind| {
        Parsed::Request(AutomationRequest {
            exclusive,
            kind,
        })
    };

    // `RADV` is the only verb that carries an argument but no group, so it is
    // matched first; like every verb it must be upper case (BACKLOG.md BUG-007).
    if raw_verb == "RADV" {
        let Some(arg) = parts.next() else {
            return Parsed::Error("arg");
        };
        if parts.next().is_some() {
            return Parsed::Error("arg");
        }
        let Ok(ticks) = arg.parse::<u32>() else {
            return Parsed::Error("arg");
        };
        return request(RequestKind::Radv { ticks });
    }

    // Verbs are case sensitive: only the upper-case spelling is accepted, so
    // `input` or `tick now` answer `ERR unknown` (BACKLOG.md BUG-007).
    match raw_verb {
        "INPUT" => {
            let Some(name) = parts.next() else {
                return Parsed::Error("arg");
            };
            let args: Vec<&str> = parts.collect();
            match args.len() {
                // `INPUT <name> <hex>`: applies on the next tick.
                1 => request(RequestKind::Input {
                    name: name.to_string(),
                    offset: None,
                    hex: args[0].to_string(),
                }),
                // `INPUT <name> <offset> <hex>`: reserved input, `rtps 0` only.
                2 => {
                    let Ok(offset) = args[0].parse::<u32>() else {
                        return Parsed::Error("arg");
                    };
                    if offset == 0 || offset > u32::from(u16::MAX) {
                        return Parsed::Error("range");
                    }
                    request(RequestKind::Input {
                        name: name.to_string(),
                        offset: Some(offset as u16),
                        hex: args[1].to_string(),
                    })
                }
                _ => Parsed::Error("arg"),
            }
        }
        "POUT" => {
            let Some(name) = parts.next() else {
                return Parsed::Error("arg");
            };
            if parts.next().is_some() {
                return Parsed::Error("arg");
            }
            request(RequestKind::Bind {
                name: name.to_string(),
            })
        }
        "READ" => {
            let Some(target) = parts.next() else {
                return Parsed::Error("arg");
            };
            if parts.next().is_some() {
                return Parsed::Error("arg");
            }
            if let Some((name, bit)) = target.split_once(':') {
                if name.is_empty() {
                    return Parsed::Error("arg");
                }
                let Ok(bit) = bit.parse::<u32>() else {
                    return Parsed::Error("arg");
                };
                request(RequestKind::ReadBit {
                    name: name.to_string(),
                    bit,
                })
            } else if target.contains(',') {
                let mut coords = target.split(',');
                let (Some(x), Some(y), Some(z)) = (coords.next(), coords.next(), coords.next())
                else {
                    return Parsed::Error("arg");
                };
                if coords.next().is_some() {
                    return Parsed::Error("arg");
                }
                let (Ok(x), Ok(y), Ok(z)) =
                    (x.parse::<i32>(), y.parse::<i32>(), z.parse::<i32>())
                else {
                    return Parsed::Error("arg");
                };
                request(RequestKind::ReadPos {
                    pos: BlockPos::new(x, y, z),
                })
            } else {
                Parsed::Error("arg")
            }
        }
        "TICK" => {
            let Some(what) = parts.next() else {
                return Parsed::Error("arg");
            };
            if parts.next().is_some() {
                return Parsed::Error("arg");
            }
            // `TICK STEP` was removed; anything but an exact `NOW` is unknown.
            if what == "NOW" {
                request(RequestKind::TickNow)
            } else {
                Parsed::Error("unknown")
            }
        }
        // `PIN`, `SUB`, `UNSUB`, `EVENT`, `USE` and `SET` are gone, and so is
        // every other verb that is not in the table.
        _ => Parsed::Error("unknown"),
    }
}

/// Encodes a bit vector as the protocol's hex word.
///
/// The vector is treated as one unsigned integer: the first bit is the most
/// significant one, and a width that is not a multiple of four is padded at
/// the front of the whole value (BACKLOG.md §7.6). A `matrix` group pads every
/// row to a whole number of hex digits, exactly like the memory files do.
pub fn encode_hex(bits: &[bool], matrix: Option<MatrixShape>) -> String {
    // A zero-width port always reports `0`; an empty string would produce
    // `OUTPUT <name> <tick> `, which no script can parse (BACKLOG.md BUG-008).
    if matrix.is_none() && bits.is_empty() {
        return "0".to_string();
    }
    let padded = match matrix {
        Some(shape) => {
            let pad = (4 - shape.first % 4) % 4;
            let mut padded = Vec::with_capacity(shape.padded_bits());
            for row in 0..shape.rows {
                padded.extend(std::iter::repeat_n(false, pad));
                let start = row * shape.first;
                match bits.get(start..start + shape.first) {
                    Some(row_bits) => padded.extend_from_slice(row_bits),
                    None => padded.extend(std::iter::repeat_n(false, shape.first)),
                }
            }
            padded
        }
        None => bits.to_vec(),
    };

    let mut hex = String::with_capacity(padded.len() / 4 + 1);
    let leftover = padded.len() % 4;
    let mut index = 0;
    if leftover != 0 {
        let mut value = 0u8;
        for bit in &padded[..leftover] {
            value = (value << 1) | u8::from(*bit);
        }
        hex.push(char::from_digit(u32::from(value), 16).unwrap());
        index = leftover;
    }
    while index < padded.len() {
        let mut value = 0u8;
        for bit in &padded[index..index + 4] {
            value = (value << 1) | u8::from(*bit);
        }
        hex.push(char::from_digit(u32::from(value), 16).unwrap());
        index += 4;
    }
    hex
}

/// Decodes a protocol hex word into the group's bit vector.
///
/// The script side is strict (BACKLOG.md §7.6): the number of digits must be
/// exactly the one the group needs, `ceil(width / 4)` for a plain list or the
/// per-row padded width for a `matrix` list, and every digit must be
/// hexadecimal. `bits` is the group width and is ignored for `matrix` groups.
pub fn decode_hex(hex: &str, bits: usize, matrix: Option<MatrixShape>) -> Result<Vec<bool>, ()> {
    // A zero-width port carries no data at all: its word is the single digit
    // `0`, so the line protocol always has a third argument (BACKLOG.md
    // BUG-008).
    if matrix.is_none() && bits == 0 {
        return if hex == "0" { Ok(Vec::new()) } else { Err(()) };
    }
    let expected_digits = match matrix {
        Some(shape) => shape.row_digits() * shape.rows,
        None => bits.div_ceil(4),
    };
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) || hex.len() != expected_digits {
        return Err(());
    }

    let mut raw = Vec::with_capacity(expected_digits * 4);
    for ch in hex.chars() {
        let digit = ch.to_digit(16).ok_or(())?;
        for shift in (0..4).rev() {
            raw.push(digit & (1 << shift) != 0);
        }
    }

    match matrix {
        None => Ok(right_align(&raw, bits)),
        Some(shape) => {
            // Strip the per-row padding again so the result lines up with the
            // group's points.
            let pad = (4 - shape.first % 4) % 4;
            let row_bits = pad + shape.first;
            let mut out = Vec::with_capacity(shape.first * shape.rows);
            for row in 0..shape.rows {
                let base = row * row_bits + pad;
                out.extend_from_slice(&raw[base..base + shape.first]);
            }
            Ok(out)
        }
    }
}

/// How reading from a connection ended.
enum ReadOutcome {
    /// Everything that is there has been consumed; more may arrive later.
    Ok,
    /// The peer closed the connection.
    Eof,
    /// Read error; the connection is unusable.
    Io,
    /// A partial line grew past [`MAX_LINE_LEN`].
    LineTooLong,
}

/// Reads every complete line currently available on the connection.
fn read_lines(conn: &mut AutomationConn, lines: &mut Vec<Vec<u8>>) -> ReadOutcome {
    loop {
        let buf = match conn.reader.fill_buf() {
            Ok(buf) => buf,
            Err(err) if err.kind() == ErrorKind::WouldBlock => return ReadOutcome::Ok,
            Err(_) => return ReadOutcome::Io,
        };
        if buf.is_empty() {
            return ReadOutcome::Eof;
        }
        match buf.iter().position(|&byte| byte == b'\n') {
            Some(newline) => {
                if conn.partial.len() + newline > MAX_LINE_LEN {
                    return ReadOutcome::LineTooLong;
                }
                conn.partial.extend_from_slice(&buf[..newline]);
                let mut line = std::mem::take(&mut conn.partial);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                lines.push(line);
                conn.reader.consume(newline + 1);
            }
            None => {
                let len = buf.len();
                conn.partial.extend_from_slice(buf);
                conn.reader.consume(len);
                if conn.partial.len() > MAX_LINE_LEN {
                    return ReadOutcome::LineTooLong;
                }
            }
        }
    }
}
