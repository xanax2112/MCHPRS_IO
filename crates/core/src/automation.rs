//! Automation bridge: a plain-text TCP protocol (default port 25585) that
//! lets external scripts read and drive the redstone IO elements (buttons,
//! levers, pressure plates, lamps, trapdoors, note blocks) of running
//! redpiler circuits, and register whole lattices of IO elements as
//! bit-vector "data ports" (`PIN`/`POUT`) that can be injected and observed
//! atomically within a single tick.
//!
//! The wire format is documented in `src/protocol.txt` in the repository
//! root.

use crate::config::CONFIG;
use crate::plot::PLOT_BLOCK_HEIGHT;
use mchprs_blocks::BlockPos;
use mchprs_redpiler::backend::{IoElement, IoEvent};
use rustc_hash::{FxHashMap, FxHashSet};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use tracing::{debug, warn};

/// The maximum length of a single request line or response line.
const MAX_LINE_LEN: usize = 4096;

/// The maximum number of lattice points a data port may span. The packed
/// state of a port takes `ceil(N / 4)` hex characters, so this also keeps
/// `INPUT`/`output` lines within `MAX_LINE_LEN`.
const MAX_PORT_POINTS: i64 = 16_000;

/// Protocol reserved words that cannot be used as data port names.
const RESERVED_NAMES: [&str; 16] = [
    "READ", "USE", "SET", "SUB", "UNSUB", "PIN", "POUT", "INPUT", "OUTPUT", "WATCH", "UNWATCH",
    "HELLO", "OK", "ERR", "WARN", "EVENT",
];

/// The kind of a registered data port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPortKind {
    /// An input port: the script drives a lattice of input elements
    /// (levers/pressure plates) with `INPUT`.
    Pin,
    /// An output port: the server pushes the packed state of a lattice of
    /// output elements (lamps/trapdoors/note blocks) with `OUTPUT` lines.
    Pout,
}

/// A registered data port: a lattice of IO elements addressed by name.
///
/// The lattice is defined by an origin `(x0, y0, z0)`, positive extents
/// `(nx, ny, nz)` and per-axis offsets `(dx, dy, dz)`. Point `(i, j, k)` is
/// at `(x0 + i*dx, y0 + j*dy, z0 + k*dz)` and carries bit index
/// `j + k*ny + i*ny*nz`; bit 0 is the most significant bit on the wire.
/// Bits advance along `y` fastest, then `z`, then `x` slowest, and `y` runs in
/// the decreasing direction while `x` and `z` increase.
/// All lattice points and the clock element lie in a single plot.
#[derive(Debug, Clone)]
pub struct DataPort {
    pub name: String,
    pub kind: DataPortKind,
    /// Whether a separate clock element drives the transfer timing.
    pub en_clk: bool,
    /// The clock element position. `(0, 0, 0)` when `en_clk` is false.
    pub clk: BlockPos,
    pub x0: i32,
    pub y0: i32,
    pub z0: i32,
    pub nx: i32,
    pub ny: i32,
    pub nz: i32,
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
    /// The lattice points in bit order (bit 0 first).
    pub points: Vec<BlockPos>,
    /// The plot containing the lattice.
    pub plot_x: i32,
    pub plot_z: i32,
}

impl DataPort {
    /// The number of bits the port carries.
    pub fn n_points(&self) -> usize {
        self.points.len()
    }

    /// The number of hex characters in the packed state (`ceil(N / 4)`).
    pub fn hex_len(&self) -> usize {
        (self.n_points() + 3) / 4
    }

    /// A position inside the plot that owns the port; used for routing.
    pub fn anchor(&self) -> BlockPos {
        self.points[0]
    }
}

/// The raw coordinate block of a `PIN`/`POUT` line, before semantic
/// validation.
struct PortSpec {
    en_clk: bool,
    clk: BlockPos,
    x0: i32,
    y0: i32,
    z0: i32,
    nx: i32,
    ny: i32,
    nz: i32,
    dx: i32,
    dy: i32,
    dz: i32,
}

/// A request from an automation client that is routed to a plot thread.
#[derive(Debug, Clone)]
pub enum AutomationRequest {
    /// Read the current state of the IO element at `pos`.
    Read { pos: BlockPos },
    /// Simulate a right-click on the button or lever at `pos`.
    Use { pos: BlockPos },
    /// Set the absolute powered state of the lever or pressure plate at `pos`.
    Set { pos: BlockPos, powered: bool },
    /// Subscribe to push `EVENT` lines for the output element at `pos`.
    Sub { pos: BlockPos },
    /// Register an input data port.
    Pin { port: DataPort },
    /// Register an output data port.
    Pout { port: DataPort },
    /// Inject a packed bit vector into a previously registered input port
    /// in a single tick.
    Input { port: DataPort, bits: Vec<bool> },
}

impl AutomationRequest {
    pub fn pos(&self) -> BlockPos {
        match self {
            AutomationRequest::Read { pos }
            | AutomationRequest::Use { pos }
            | AutomationRequest::Set { pos, .. }
            | AutomationRequest::Sub { pos } => *pos,
            AutomationRequest::Pin { port } | AutomationRequest::Pout { port } => port.anchor(),
            AutomationRequest::Input { port, .. } => port.anchor(),
        }
    }
}

/// The reply a plot thread sends back after handling an [`AutomationRequest`].
#[derive(Debug, Clone)]
pub enum AutomationResponse {
    /// `READ` result: the element at `pos` and its current state. The
    /// trailing `u8` is the internal output power; protocol v5 does not
    /// print it on the wire.
    Read(BlockPos, IoElement, bool, u8),
    /// `USE` succeeded.
    UseOk(BlockPos),
    /// `SET` succeeded.
    SetOk(BlockPos, bool),
    /// `SUB` succeeded; the connection is registered for `pos`. The trailing
    /// fields carry the element's current state at registration time so the
    /// client learns the initial value without an extra `READ`.
    SubOk(BlockPos, IoElement, bool, u8),
    /// `PIN` registration succeeded; the server stores the port.
    PinOk(DataPort),
    /// `POUT` registration succeeded; the server stores the port.
    PoutOk(DataPort),
    /// `INPUT` injection succeeded.
    InputOk(String),
    /// No redpiler circuit is running on the plot containing `pos`.
    Inactive(BlockPos),
    /// No IO element of the running circuit exists at `pos`.
    NoIoElement(BlockPos),
    /// The element at `pos` is an input element, which cannot be watched.
    NotOutput(BlockPos),
    /// The operation could not be applied to the element at `pos`.
    OperationFailed(BlockPos),
    /// A data port operation failed; the payload is the short error code
    /// printed after `ERR` on the wire (`TYPE`, `CLK`, ...).
    PortError(&'static str),
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
    /// An automation connection closed; plots holding its pout ports must
    /// drop them so they stop sampling and pushing state.
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
}

/// Manages the automation TCP listener and all its connections.
pub struct AutomationServer {
    listener: TcpListener,
    next_conn_id: u64,
    connections: FxHashMap<u64, AutomationConn>,
    /// Positions subscribed by each connection.
    watches: FxHashMap<u64, FxHashSet<BlockPos>>,
    /// Connections that receive push events for each position.
    watchers: FxHashMap<BlockPos, FxHashSet<u64>>,
    /// Data ports registered by each connection, keyed by name.
    data_ports: FxHashMap<u64, FxHashMap<String, DataPort>>,
    /// Disconnects recorded by [`AutomationServer::drop_connection`] that
    /// still need to be reported to plot threads.
    pending_disconnects: Vec<(u64, i32, i32)>,
}

impl AutomationServer {
    /// Binds the automation listener to `127.0.0.1:port`. Loopback only: the
    /// automation bridge is meant for local scripts, not remote clients.
    pub fn bind(port: i64) -> std::io::Result<Self> {
        let port = u16::try_from(port).unwrap_or(25585);
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        Ok(AutomationServer {
            listener,
            next_conn_id: 0,
            connections: FxHashMap::default(),
            watches: FxHashMap::default(),
            watchers: FxHashMap::default(),
            data_ports: FxHashMap::default(),
            pending_disconnects: Vec::new(),
        })
    }

    /// Accepts new connections, reads and parses requests from all
    /// connections, and returns the requests that need to be routed to a
    /// plot. Requests that can be handled locally (`UNSUB`, data port
    /// registration errors, `INPUT` bookkeeping) are answered immediately by
    /// queueing a response line.
    pub fn poll(&mut self) -> Vec<AutomationAction> {
        self.accept_new();

        let mut actions = Vec::new();
        let mut local: Vec<(u64, LocalAction)> = Vec::new();
        let mut closed = Vec::new();

        for (conn_id, conn) in &mut self.connections {
            let conn_id = *conn_id;
            let mut lines = Vec::new();
            match poll_lines(conn, &mut lines) {
                Ok(true) => {}
                Ok(false) | Err(()) => {
                    closed.push(conn_id);
                    continue;
                }
            }
            for line in lines {
                if line.is_empty() {
                    continue;
                }
                match parse_request(&line) {
                    Some(ParseResult::Route(pos, request)) => {
                        actions.push(AutomationAction::Route {
                            conn_id,
                            pos,
                            request,
                        });
                    }
                    Some(ParseResult::Unsub(pos)) => {
                        local.push((conn_id, LocalAction::Unsub(pos)));
                    }
                    Some(ParseResult::Register { kind, name, spec }) => {
                        match build_port(&name, kind, spec) {
                            Err(code) => local.push((conn_id, LocalAction::PortError(code))),
                            Ok(port) => {
                                let dup = self
                                    .data_ports
                                    .get(&conn_id)
                                    .is_some_and(|ports| ports.contains_key(&name));
                                if dup {
                                    local.push((conn_id, LocalAction::PortError("DUP")));
                                } else {
                                    let pos = port.anchor();
                                    let request = match kind {
                                        DataPortKind::Pin => AutomationRequest::Pin { port },
                                        DataPortKind::Pout => AutomationRequest::Pout { port },
                                    };
                                    actions.push(AutomationAction::Route {
                                        conn_id,
                                        pos,
                                        request,
                                    });
                                }
                            }
                        }
                    }
                    Some(ParseResult::Input { name, hex }) => {
                        match self.data_ports.get(&conn_id).and_then(|ports| ports.get(&name)) {
                            None => local.push((conn_id, LocalAction::PortError("NAME"))),
                            Some(port) => {
                                let port = port.clone();
                                match parse_hex_bits(&hex, &port) {
                                    None => {
                                        local.push((conn_id, LocalAction::PortError("HEX")))
                                    }
                                    Some(bits) => {
                                        let pos = port.anchor();
                                        actions.push(AutomationAction::Route {
                                            conn_id,
                                            pos,
                                            request: AutomationRequest::Input { port, bits },
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Some(ParseResult::Invalid(line)) => {
                        local.push((conn_id, LocalAction::Invalid(line)));
                    }
                    None => local.push((conn_id, LocalAction::Malformed(line))),
                }
            }
        }

        for conn_id in closed {
            self.drop_connection(conn_id);
        }
        for (conn_id, action) in local {
            match action {
                LocalAction::Unsub(pos) => {
                    self.remove_watch(conn_id, pos);
                    self.queue_line(conn_id, format!("OK UNSUB {} {} {}\n", pos.x, pos.y, pos.z));
                }
                LocalAction::PortError(code) => {
                    self.queue_line(conn_id, format!("ERR {}\n", code));
                }
                LocalAction::Invalid(line) => {
                    self.queue_line(conn_id, format!("ERR invalid request: {}\n", line));
                }
                LocalAction::Malformed(line) => {
                    self.queue_line(conn_id, format!("ERR malformed request: {}\n", line));
                }
            }
        }
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

    /// Flushes queued responses to all connections, removing dead sockets.
    pub fn flush(&mut self) {
        let mut closed = Vec::new();
        for (conn_id, conn) in &mut self.connections {
            let conn_id = *conn_id;
            while !conn.pending.is_empty() {
                match conn.writer.write(&conn.pending) {
                    Ok(0) => {
                        closed.push(conn_id);
                        break;
                    }
                    Ok(written) => {
                        conn.pending.drain(..written);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => {
                        closed.push(conn_id);
                        break;
                    }
                }
            }
        }
        for conn_id in closed {
            self.drop_connection(conn_id);
        }
    }

    /// Handles a reply from a plot thread for a previously routed request.
    pub fn handle_reply(&mut self, conn_id: u64, reply: AutomationResponse) {
        let line = match reply {
            // Protocol v5 does not expose the internal output power.
            AutomationResponse::Read(pos, element, powered, _) => format!(
                "OK READ {} {} {} {} {}\n",
                pos.x,
                pos.y,
                pos.z,
                element.code(),
                powered as u8
            ),
            AutomationResponse::UseOk(pos) => {
                format!("OK USE {} {} {}\n", pos.x, pos.y, pos.z)
            }
            AutomationResponse::SetOk(pos, powered) => {
                format!("OK SET {} {} {} {}\n", pos.x, pos.y, pos.z, powered as u8)
            }
            AutomationResponse::SubOk(pos, element, powered, _) => {
                self.watches.entry(conn_id).or_default().insert(pos);
                self.watchers.entry(pos).or_default().insert(conn_id);
                format!(
                    "OK SUB {} {} {} {} {}\n",
                    pos.x,
                    pos.y,
                    pos.z,
                    element.code(),
                    powered as u8
                )
            }
            AutomationResponse::PinOk(port) => {
                let line = format!("OK PIN {}\n", port.name);
                self.data_ports
                    .entry(conn_id)
                    .or_default()
                    .insert(port.name.clone(), port);
                line
            }
            AutomationResponse::PoutOk(port) => {
                let line = format!("OK POUT {}\n", port.name);
                self.data_ports
                    .entry(conn_id)
                    .or_default()
                    .insert(port.name.clone(), port);
                line
            }
            AutomationResponse::InputOk(name) => {
                format!("OK INPUT {}\n", name)
            }
            AutomationResponse::Inactive(pos) => {
                format!("WARN INACTIVE {} {} {}\n", pos.x, pos.y, pos.z)
            }
            AutomationResponse::NoIoElement(pos) => {
                format!("WARN NO_IO {} {} {}\n", pos.x, pos.y, pos.z)
            }
            AutomationResponse::NotOutput(pos) => {
                format!("WARN NOT_OUTPUT {} {} {}\n", pos.x, pos.y, pos.z)
            }
            AutomationResponse::OperationFailed(pos) => {
                format!("WARN FAILED {} {} {}\n", pos.x, pos.y, pos.z)
            }
            AutomationResponse::PortError(code) => {
                format!("ERR {}\n", code)
            }
        };
        self.queue_line(conn_id, line);
    }

    /// Pushes a state change event to all connections watching `event.pos`.
    ///
    /// When `automation_data_port_events` is disabled, a subscribed point
    /// that lies inside one of the connection's `POUT` data ports never
    /// pushes an `EVENT` (whether or not the port's clock fires): its state
    /// changes are only visible in the packed `OUTPUT` line. Subscribed
    /// points outside every `POUT` port are unaffected.
    pub fn push_event(&mut self, event: IoEvent) {
        let watchers: Vec<u64> = match self.watchers.get(&event.pos) {
            Some(watchers) => watchers.iter().copied().collect(),
            None => return,
        };
        // The internal output power is not exposed on the wire.
        let line = format!(
            "EVENT {} {} {} {} {}\n",
            event.pos.x,
            event.pos.y,
            event.pos.z,
            event.element.code(),
            event.powered as u8
        );
        for conn_id in watchers {
            if !CONFIG.automation_data_port_events
                && self.data_ports.get(&conn_id).is_some_and(|ports| {
                    ports.values().any(|port| {
                        port.kind == DataPortKind::Pout
                            && port.points.contains(&event.pos)
                    })
                })
            {
                continue;
            }
            self.queue_line(conn_id, line.clone());
        }
    }

    /// Pushes a packed data port state line to a connection. The `OUTPUT`
    /// keyword is upper case and the port NAME is pushed exactly as
    /// registered (case preserved).
    pub fn push_data_port_output(&mut self, conn_id: u64, name: &str, hex: &str) {
        self.queue_line(
            conn_id,
            format!("OUTPUT {} {}\n", name, hex),
        );
    }

    fn accept_new(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    debug!("Automation client connected from {}", addr);
                    let _ = stream.set_nonblocking(true);
                    let reader = match stream.try_clone() {
                        Ok(stream) => BufReader::new(stream),
                        Err(_) => {
                            warn!("Failed to clone automation socket for {}", addr);
                            continue;
                        }
                    };
                    let conn_id = self.next_conn_id;
                    self.next_conn_id += 1;
                    self.connections.insert(
                        conn_id,
                        AutomationConn {
                            writer: stream,
                            reader,
                            partial: Vec::new(),
                            pending: Vec::new(),
                        },
                    );
                    self.watches.insert(conn_id, FxHashSet::default());
                    self.data_ports.insert(conn_id, FxHashMap::default());
                    self.queue_line(conn_id, "HELLO MCHPRS AUTOMATION 0.2.1-beta\n".to_string());
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(e) => {
                    warn!("Automation listener accept failed: {}", e);
                    return;
                }
            }
        }
    }

    fn queue_line(&mut self, conn_id: u64, line: String) {
        if let Some(conn) = self.connections.get_mut(&conn_id) {
            conn.pending.extend_from_slice(line.as_bytes());
        }
    }

    fn remove_watch(&mut self, conn_id: u64, pos: BlockPos) {
        if let Some(watched) = self.watches.get_mut(&conn_id) {
            watched.remove(&pos);
        }
        let empty = match self.watchers.get_mut(&pos) {
            Some(watchers) => {
                watchers.remove(&conn_id);
                watchers.is_empty()
            }
            None => false,
        };
        if empty {
            self.watchers.remove(&pos);
        }
    }

    fn drop_connection(&mut self, conn_id: u64) {
        let watched = self.watches.remove(&conn_id).unwrap_or_default();
        for pos in watched {
            self.remove_watch(conn_id, pos);
        }
        if let Some(ports) = self.data_ports.remove(&conn_id) {
            let mut plots: FxHashSet<(i32, i32)> = FxHashSet::default();
            for port in ports.into_values() {
                plots.insert((port.plot_x, port.plot_z));
            }
            for (plot_x, plot_z) in plots {
                self.pending_disconnects.push((conn_id, plot_x, plot_z));
            }
        }
        self.connections.remove(&conn_id);
    }
}

/// A parsed request that the bridge can handle without plot involvement.
enum LocalAction {
    Unsub(BlockPos),
    /// Queue an `ERR <code>` line for a failed data port operation.
    PortError(&'static str),
    Invalid(String),
    Malformed(String),
}

/// A parsed request that must be routed to a plot.
enum ParseResult {
    Route(BlockPos, AutomationRequest),
    Unsub(BlockPos),
    /// A `PIN`/`POUT` registration line with its coordinate block.
    Register {
        kind: DataPortKind,
        name: String,
        spec: PortSpec,
    },
    /// An `INPUT` injection line.
    Input { name: String, hex: String },
    /// The opcode was not recognized; the offending line is echoed back.
    Invalid(String),
}

/// Parses one client request line. Returns `None` when the line is
/// syntactically malformed (bad coordinates, missing or extra arguments).
fn parse_request(line: &str) -> Option<ParseResult> {
    let mut parts = line.split_whitespace();
    let op = parts.next()?.to_ascii_uppercase();
    Some(match op.as_str() {
        "READ" | "USE" | "SUB" | "UNSUB" => {
            let pos = parse_pos(&mut parts)?;
            if parts.next().is_some() {
                return None;
            }
            match op.as_str() {
                "READ" => ParseResult::Route(pos, AutomationRequest::Read { pos }),
                "USE" => ParseResult::Route(pos, AutomationRequest::Use { pos }),
                "SUB" => ParseResult::Route(pos, AutomationRequest::Sub { pos }),
                "UNSUB" => ParseResult::Unsub(pos),
                _ => unreachable!(),
            }
        }
        "SET" => {
            let pos = parse_pos(&mut parts)?;
            let powered = parts.next()?.parse::<u8>().ok()? != 0;
            if parts.next().is_some() {
                return None;
            }
            ParseResult::Route(pos, AutomationRequest::Set { pos, powered })
        }
        "PIN" | "POUT" => {
            let kind = if op == "PIN" {
                DataPortKind::Pin
            } else {
                DataPortKind::Pout
            };
            let name = parts.next()?.to_string();
            let en_clk = parts.next()?.parse::<u8>().ok()? != 0;
            let cx: i32 = parts.next()?.parse().ok()?;
            let cy: i32 = parts.next()?.parse().ok()?;
            let cz: i32 = parts.next()?.parse().ok()?;
            let x0: i32 = parts.next()?.parse().ok()?;
            let y0: i32 = parts.next()?.parse().ok()?;
            let z0: i32 = parts.next()?.parse().ok()?;
            let nx: i32 = parts.next()?.parse().ok()?;
            let ny: i32 = parts.next()?.parse().ok()?;
            let nz: i32 = parts.next()?.parse().ok()?;
            let dx: i32 = parts.next()?.parse().ok()?;
            let dy: i32 = parts.next()?.parse().ok()?;
            let dz: i32 = parts.next()?.parse().ok()?;
            if parts.next().is_some() {
                return None;
            }
            ParseResult::Register {
                kind,
                name,
                spec: PortSpec {
                    en_clk,
                    clk: BlockPos::new(cx, cy, cz),
                    x0,
                    y0,
                    z0,
                    nx,
                    ny,
                    nz,
                    dx,
                    dy,
                    dz,
                },
            }
        }
        "INPUT" => {
            let name = parts.next()?.to_string();
            let hex = parts.next()?.to_string();
            if parts.next().is_some() {
                return None;
            }
            ParseResult::Input { name, hex }
        }
        _ => ParseResult::Invalid(line.to_string()),
    })
}

/// Parses three coordinate tokens into a [`BlockPos`].
fn parse_pos(parts: &mut std::str::SplitWhitespace<'_>) -> Option<BlockPos> {
    let x: i32 = parts.next()?.parse().ok()?;
    let y: i32 = parts.next()?.parse().ok()?;
    let z: i32 = parts.next()?.parse().ok()?;
    Some(BlockPos::new(x, y, z))
}

/// Whether `name` is a valid data port name: a C-style identifier of at most
/// 32 characters that is not a protocol reserved word. Names are
/// case-sensitive but reserved words match case-insensitively.
fn valid_port_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 32 {
        return false;
    }
    let bytes = name.as_bytes();
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    if !bytes.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_') {
        return false;
    }
    !RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
}

/// Validates a `PIN`/`POUT` coordinate block and builds the lattice.
/// Returns the short wire error code (`NAME`, `RANGE`, `CLK`) on failure.
fn build_port(name: &str, kind: DataPortKind, spec: PortSpec) -> Result<DataPort, &'static str> {
    if !valid_port_name(name) {
        return Err("NAME");
    }
    let PortSpec {
        en_clk,
        clk,
        x0,
        y0,
        z0,
        nx,
        ny,
        nz,
        dx,
        dy,
        dz,
    } = spec;
    if nx < 1 || ny < 1 || nz < 1 {
        return Err("RANGE");
    }
    // An extent of 1 spans a single point, so its offset must be 0; larger
    // extents need a non-zero offset. Bits advance in the +x, -y and +z
    // directions, so y counts down from the origin while x and z count up.
    if (nx == 1 && dx != 0) || (nx > 1 && dx <= 0) {
        return Err("RANGE");
    }
    if (ny == 1 && dy != 0) || (ny > 1 && dy >= 0) {
        return Err("RANGE");
    }
    if (nz == 1 && dz != 0) || (nz > 1 && dz <= 0) {
        return Err("RANGE");
    }
    let total = i64::from(nx) * i64::from(ny) * i64::from(nz);
    if total > MAX_PORT_POINTS {
        return Err("RANGE");
    }
    if !en_clk && clk != BlockPos::new(0, 0, 0) {
        return Err("CLK");
    }

    let plot_x = x0 >> 9;
    let plot_z = z0 >> 9;
    let mut points = Vec::with_capacity(total as usize);
    // Bit order is y fastest, then z, then x (index j + k*ny + i*ny*nz).
    for i in 0..nx {
        for k in 0..nz {
            for j in 0..ny {
                let x64 = i64::from(x0) + i64::from(i) * i64::from(dx);
                let y64 = i64::from(y0) + i64::from(j) * i64::from(dy);
                let z64 = i64::from(z0) + i64::from(k) * i64::from(dz);
                if x64 < i64::from(i32::MIN)
                    || x64 > i64::from(i32::MAX)
                    || y64 < i64::from(i32::MIN)
                    || y64 > i64::from(i32::MAX)
                    || z64 < i64::from(i32::MIN)
                    || z64 > i64::from(i32::MAX)
                {
                    return Err("RANGE");
                }
                let x = x64 as i32;
                let y = y64 as i32;
                let z = z64 as i32;
                if y < 0 || y >= PLOT_BLOCK_HEIGHT {
                    return Err("RANGE");
                }
                if (x >> 9) != plot_x || (z >> 9) != plot_z {
                    return Err("RANGE");
                }
                points.push(BlockPos::new(x, y, z));
            }
        }
    }

    if en_clk {
        if clk.y < 0 || clk.y >= PLOT_BLOCK_HEIGHT {
            return Err("RANGE");
        }
        if (clk.x >> 9) != plot_x || (clk.z >> 9) != plot_z {
            return Err("RANGE");
        }
        // The clock element must sit outside the lattice bounding box.
        let min_x = points.iter().map(|p| p.x).min().unwrap();
        let max_x = points.iter().map(|p| p.x).max().unwrap();
        let min_y = points.iter().map(|p| p.y).min().unwrap();
        let max_y = points.iter().map(|p| p.y).max().unwrap();
        let min_z = points.iter().map(|p| p.z).min().unwrap();
        let max_z = points.iter().map(|p| p.z).max().unwrap();
        if clk.x >= min_x
            && clk.x <= max_x
            && clk.y >= min_y
            && clk.y <= max_y
            && clk.z >= min_z
            && clk.z <= max_z
        {
            return Err("CLK");
        }
    }

    Ok(DataPort {
        name: name.to_string(),
        kind,
        en_clk,
        clk,
        x0,
        y0,
        z0,
        nx,
        ny,
        nz,
        dx,
        dy,
        dz,
        points,
        plot_x,
        plot_z,
    })
}

/// Decodes the packed hex state of a data port into its bit vector.
/// Returns `None` when the hex string has the wrong length or contains
/// non-hex characters.
fn parse_hex_bits(hex: &str, port: &DataPort) -> Option<Vec<bool>> {
    if hex.len() != port.hex_len() {
        return None;
    }
    let mut bits = Vec::with_capacity(port.n_points());
    for c in hex.chars() {
        let value = c.to_digit(16)? as u8;
        for shift in (0..4).rev() {
            if bits.len() < port.n_points() {
                bits.push(value & (1 << shift) != 0);
            }
        }
    }
    Some(bits)
}

/// Reads complete lines from the connection's buffer.
/// Returns `Ok(true)` when more data may arrive later, `Ok(false)` on EOF,
/// and `Err(())` on read errors or when a line exceeds `MAX_LINE_LEN`.
fn poll_lines(conn: &mut AutomationConn, lines: &mut Vec<String>) -> Result<bool, ()> {
    loop {
        let buf = match conn.reader.fill_buf() {
            Ok(buf) => buf,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(true),
            Err(_) => return Err(()),
        };
        if buf.is_empty() {
            return Ok(false);
        }
        match buf.iter().position(|&byte| byte == b'\n') {
            Some(newline_idx) => {
                conn.partial.extend_from_slice(&buf[..newline_idx]);
                if conn.partial.len() > MAX_LINE_LEN {
                    return Err(());
                }
                lines.push(String::from_utf8_lossy(&conn.partial).trim().to_string());
                conn.partial.clear();
                conn.reader.consume(newline_idx + 1);
            }
            None => {
                let len = buf.len();
                conn.partial.extend_from_slice(buf);
                if conn.partial.len() > MAX_LINE_LEN {
                    return Err(());
                }
                conn.reader.consume(len);
            }
        }
    }
}
