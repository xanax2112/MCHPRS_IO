use super::{ReloadOutcome, database, worldedit, Plot, PlotWorld};
use crate::player::{Gamemode, PacketSender, PlayerPos};
use crate::server::{get_version_string, Message};
use mchprs_blocks::items::ItemStack;
use mchprs_network::packets::clientbound::{
    CCommands, CCommandsNode as Node, CDeclareCommandsNodeParser as Parser, ClientBoundPacket,
};
use mchprs_network::packets::PacketEncoder;
use mchprs_redpiler::CompilerOptions;
use mchprs_save_data::plot_data::{Tps, WorldSendRate};
use mchprs_text::TextComponent;
use std::ops::Add;
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Instant;
use tracing::{debug, info, warn};

// Parses a relative or absolute coordinate relative to a reference coordinate
fn parse_relative_coord<F: FromStr + Add + Add<Output = F>>(
    coord: &str,
    ref_coord: F,
) -> Result<F, <F as FromStr>::Err> {
    if coord == "~" {
        Ok(ref_coord)
    } else if let Some(offset_str) = coord.strip_prefix('~') {
        offset_str.parse::<F>().map(|x| ref_coord + x)
    } else {
        coord.parse::<F>()
    }
}

impl Plot {
    /// Handles a command that starts with `/plot` or `/p`
    fn handle_plot_command(&mut self, player: usize, command: &str, args: &[&str]) {
        let (plot_x, plot_z) = self.players[player].pos.plot_pos();

        let permission_node = match command {
            "info" | "i" => "plots.info",
            "claim" | "c" => "plots.claim",
            "auto" | "a" => "plots.auto",
            "middle" => "plots.middle",
            "visit" | "v" => "plots.visit",
            "teleport" | "tp" => "plots.visit",
            "lock" | "unlock" => "plots.lock",
            "sel" | "select" => "plots.select",
            _ => {
                self.players[player].send_error_message("Invalid argument for /plot");
                return;
            }
        };
        if !self.players[player].has_permission(permission_node) {
            self.players[player].send_no_permission_message();
            return;
        }

        match command {
            "info" | "i" => {
                if let Some(owner) = database::get_plot_owner(plot_x, plot_z) {
                    self.players[player].send_system_message(&format!(
                        "Plot owner is: {}",
                        database::get_cached_username(owner.clone()).unwrap_or(owner)
                    ));
                } else {
                    self.players[player].send_system_message("Plot is not owned by anyone.");
                }
            }
            "claim" | "c" => {
                if database::is_claimed(plot_x, plot_z).unwrap() {
                    self.players[player].send_system_message("Plot is already claimed!");
                } else {
                    self.claim_plot(plot_x, plot_z, player);
                }
            }
            "auto" | "a" => {
                let mut start = (0, 0);
                for _ in 0..i32::MAX {
                    if database::is_claimed(start.0, start.1).unwrap() {
                        start = Plot::get_next_plot(start.0, start.1);
                    } else {
                        self.claim_plot(start.0, start.1, player);
                        break;
                    }
                }
            }
            "middle" => {
                let center = Plot::get_center(plot_x, plot_z);
                self.players[player].teleport(PlayerPos::new(center.0, 64.0, center.1));
            }
            "visit" | "v" => {
                if !(1..=2).contains(&args.len()) {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return;
                }

                let idx = if args.len() == 2 {
                    match args[1].parse::<usize>() {
                        Ok(idx) => idx.wrapping_sub(1),
                        Err(_) => {
                            self.players[player].send_error_message("Unable to parse index");
                            return;
                        }
                    }
                } else {
                    0
                };

                let plots = database::get_owned_plots(args[0]);
                if !plots.is_empty() {
                    if let Some(&(plot_x, plot_z)) = plots.get(idx) {
                        let center = Plot::get_center(plot_x, plot_z);
                        self.players[player].teleport(PlayerPos::new(center.0, 64.0, center.1));
                    } else {
                        self.players[player]
                            .send_system_message(&format!("Plot range (1, {}).", plots.len()));
                    }
                } else {
                    self.players[player]
                        .send_system_message(&format!("{} does not own any plots.", args[0]));
                }
            }
            "teleport" | "tp" => {
                if args.len() != 2 {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return;
                }

                let new_plot_x;
                let new_plot_z;
                if let Ok(x_arg) = parse_relative_coord(args[0], plot_x) {
                    new_plot_x = x_arg;
                } else {
                    self.players[player].send_error_message("Unable to parse x coordinate!");
                    return;
                }
                if let Ok(z_arg) = parse_relative_coord(args[1], plot_z) {
                    new_plot_z = z_arg;
                } else {
                    self.players[player].send_error_message("Unable to parse z coordinate!");
                    return;
                }

                let center = Plot::get_center(new_plot_x, new_plot_z);
                self.players[player].teleport(PlayerPos::new(center.0, 64.0, center.1));
            }
            "lock" => {
                if self.locked_players.insert(self.players[player].entity_id) {
                    let PlotWorld { x, z, .. } = self.world;
                    let res = format!("Locked to plot ({}, {}). Use '/p unlock' to unlock.", x, z);
                    self.players[player].send_system_message(&res);
                } else {
                    self.players[player]
                        .send_system_message("You are already locked to this plot.");
                }
            }
            "unlock" => {
                if self.locked_players.remove(&self.players[player].entity_id) {
                    self.players[player].send_system_message("You are now unlocked.");
                } else {
                    self.players[player].send_system_message("You are not locked to this plot.");
                }
            }
            "select" | "sel" => {
                let corners = self.world.get_corners();
                self.players[player].worldedit_set_first_position(corners.0);
                self.players[player].worldedit_set_second_position(corners.1);
            }
            _ => self.players[player].send_error_message("Invalid argument for /plot"),
        }
    }

    /// Handles a command that starts with `/redpiler` or `/rp`
    fn handle_redpiler_command(&mut self, player: usize, command: &str, args: &[&str]) {
        match command {
            "compile" | "c" => {
                let start_time = Instant::now();
                let args = args.join(" ");
                let options = CompilerOptions::parse(&args);

                if options.optimize {
                    let msg = "Redpiler optimization is highly unstable and can break builds. Use with caution!";
                    warn!("{}", msg);
                    self.players[player].send_system_message(msg);
                }

                self.reset_redpiler();
                self.start_redpiler(options, Some(player));
                self.publish_world();

                debug!("Compile took {:?}", start_time.elapsed());
            }
            "inspect" | "i" => {
                let player = &self.players[player];
                let pos = worldedit::ray_trace_block(
                    &self.world,
                    player.pos,
                    player.pitch as f64,
                    player.yaw as f64,
                    10.0,
                );
                let Some(pos) = pos else {
                    player.send_error_message("Trace failed");
                    return;
                };
                self.redpiler.inspect(pos);
            }
            "reset" | "r" => {
                self.reset_redpiler();
                self.publish_world();
            }
            _ => self.players[player].send_error_message("Invalid argument for /redpiler"),
        }
    }


    /// Executes a command line sent by an automation script (`/...`).
    ///
    /// The script has no player entity, so only commands that do not depend on
    /// player state are supported. Everything else is rejected with `ERR CMD`.
    /// The dispatcher below mirrors the player path for the supported subset;
    /// it deliberately does not reuse `handle_command` because that function is
    /// written entirely in terms of `self.players[player]`.
    /// Runs one `/` command typed into the server terminal (BUG-031).
    ///
    /// Commands that need a player target are rejected; everything else
    /// behaves like the in-game command (including `/rtps 0`, which takes the
    /// automation bridge into exclusive manual stepping).
    pub(super) fn handle_console_command(&mut self, line: &str) {
        let line = line.strip_prefix('/').unwrap_or(line);
        let mut parts = line.split_whitespace();
        let Some(command) = parts.next() else {
            return;
        };
        let args: Vec<&str> = parts.collect();
        match command {
            "stop" => {
                crate::console::ok("Shutting down...");
                let _ = self.message_sender.send(Message::Shutdown);
            }
            "io" => match args.first().copied() {
                Some("list") => {
                    let summary = self.io_summary();
                    crate::console::ok(&summary);
                }
                Some("reload") | Some("r") => match self.reload_io() {
                    ReloadOutcome::CompiledIoOnly => crate::console::error(
                        "/io reload is not available while a circuit compiled with -i is running.",
                    ),
                    ReloadOutcome::Reloaded { loaded, rejected } => {
                        crate::console::ok(&format!("Reloaded {} IO group(s).", loaded));
                        for message in rejected {
                            crate::console::error(&message);
                        }
                    }
                },
                _ => crate::console::error("Usage: /io <list|reload>"),
            },
            "memory" => match args.first().copied() {
                Some("reload") | Some("r") => match self.reload_memories() {
                    ReloadOutcome::CompiledIoOnly => crate::console::error(
                        "/memory reload is not available while a circuit compiled with -i is running.",
                    ),
                    ReloadOutcome::Reloaded { loaded, rejected } => {
                        crate::console::ok(&format!(
                            "Reloaded {} memory definition(s).",
                            loaded
                        ));
                        for message in rejected {
                            crate::console::error(&message);
                        }
                    }
                },
                _ => crate::console::error("Usage: /memory reload"),
            },
            "rtps" => {
                let Some(arg) = args.first() else {
                    crate::console::error("Usage: /rtps <n | unlimited>");
                    return;
                };
                let tps = if let Ok(value) = arg.parse::<f32>() {
                    if !value.is_finite() || value < 0.0 {
                        crate::console::error("RTPS must be a finite, non-negative number.");
                        return;
                    }
                    Tps::Limited(value)
                } else if !arg.is_empty() && "unlimited".starts_with(arg) {
                    Tps::Unlimited
                } else {
                    crate::console::error("Unable to parse rtps!");
                    return;
                };
                self.apply_rtps(tps);
                self.on_player_rtps(&args);
                crate::console::ok("The rtps was successfully set.");
            }
            "tick" => match args.first().copied() {
                None => match self.current_tick() {
                    Some(tick) => crate::console::ok(&format!("Tick: {}", tick)),
                    None => crate::console::error("No circuit is compiled, so there is no tick."),
                },
                Some("seq") if args.len() == 1 => match self.start_seq_mode() {
                    Ok(()) => crate::console::ok(
                        "Reservation mode enabled: scripts disconnected, rtps set to 0.",
                    ),
                    Err(code) => crate::console::error(&self.seq_error(code)),
                },
                Some("freeze") if args.len() == 1 => {
                    self.freeze_rtps();
                    crate::console::ok("Tick rate frozen (rtps 0).");
                }
                Some("unfreeze") if args.len() == 1 => match self.unfreeze_rtps() {
                    Some(_) => crate::console::ok("Tick rate restored."),
                    None => crate::console::error("Tick rate was not frozen."),
                },
                Some("step") => {
                    let step: u64 = match args.get(1).and_then(|arg| arg.parse().ok()) {
                        Some(step) => step,
                        None => {
                            crate::console::error("Usage: /tick step <even number>");
                            return;
                        }
                    };
                    if step == 0 || step % 2 != 0 {
                        crate::console::error(
                            "/tick step only accepts even numbers (/tick step 2 = /radv 1).",
                        );
                        return;
                    }
                    self.advance_ticks(step / 2);
                    crate::console::ok(&format!("Advanced {} tick(s).", step / 2));
                }
                Some("rate") => {
                    if let Some(arg) = args.get(1) {
                        let tps = if let Ok(tps) = arg.parse::<f32>() {
                            Tps::Limited(tps)
                        } else if !arg.is_empty() && "unlimited".starts_with(arg) {
                            Tps::Unlimited
                        } else {
                            crate::console::error("Usage: /tick rate [rtps]");
                            return;
                        };
                        self.apply_rtps(tps);
                    } else {
                        self.apply_rtps(Tps::Limited(20.0));
                    }
                    crate::console::ok("The rtps was successfully set.");
                }
                Some(_) => crate::console::error("Usage: /tick [seq]"),
            },
            "radv" | "radvance" => {
                let Some(ticks) = args.first().and_then(|arg| arg.parse::<u32>().ok()) else {
                    crate::console::error("Usage: /radv <ticks>");
                    return;
                };
                self.advance_ticks(u64::from(ticks));
                crate::console::ok(&format!("Advanced {} tick(s).", ticks));
            }
            "redpiler" | "rp" => match self.execute_automation_command("redpiler", &args) {
                Ok(()) => crate::console::ok("Done."),
                Err(code) => crate::console::error(&format!("ERR {}", code)),
            },
            "version" => crate::console::ok(&get_version_string()),
            _ => crate::console::error(crate::console::NEEDS_PLAYER),
        }
    }

    fn execute_automation_command(
        &mut self,
        command: &str,
        args: &[&str],
    ) -> Result<(), &'static str> {
        match command {
            "redpiler" | "rp" => {
                let sub = args.first().copied().ok_or("arg")?;
                match sub {
                    "compile" | "c" => {
                        let options = CompilerOptions::parse(&args[1..].join(" "));
                        self.reset_redpiler();
                        // No source player: the compile runs silently, exactly
                        // like an automatic compile.
                        self.start_redpiler(options, None);
                        self.publish_world();
                        Ok(())
                    }
                    "reset" | "r" => {
                        self.reset_redpiler();
                        self.publish_world();
                        Ok(())
                    }
                    _ => Err("unknown"),
                }
            }
            "stop" => {
                let _ = self.message_sender.send(Message::Shutdown);
                Ok(())
            }
            "radv" | "radvance" => {
                let ticks: u32 = args.first().and_then(|arg| arg.parse().ok()).ok_or("arg")?;
                self.advance_ticks(u64::from(ticks));
                Ok(())
            }
            "memory" => match args.first().copied() {
                Some("reload") | Some("r") => match self.reload_memories() {
                    ReloadOutcome::CompiledIoOnly => Err("mode"),
                    ReloadOutcome::Reloaded { rejected, .. } => {
                        for message in rejected {
                            warn!("{}", message);
                        }
                        Ok(())
                    }
                },
                _ => Err("arg"),
            },
            "worldsendrate" | "wsr" => {
                let hertz: f32 = args.first().and_then(|arg| arg.parse().ok()).ok_or("arg")?;
                if !hertz.is_finite() || !(0.0..=1000.0).contains(&hertz) {
                    return Err("arg");
                }
                self.world.world_send_rate = WorldSendRate(hertz);
                self.world.pending_sounds.clear();
                self.last_world_send_time = Instant::now();
                Ok(())
            }
            _ => Err("unknown"),
        }
    }

    /// Chat/console text for a [`Plot::start_seq_mode`] failure (BACKLOG.md
    /// BUG-004).
    fn seq_error(&self, code: &'static str) -> String {
        match code {
            "nocompile" => "/tick seq needs a compiled circuit; compile first.".to_string(),
            _ => "/tick seq is not available in performance mode.".to_string(),
        }
    }

    // Returns true if packets should stop being handled
    pub(super) fn handle_command(
        &mut self,
        player: usize,
        command: &str,
        mut args: Vec<&str>,
    ) -> bool {
        info!(
            "{} issued command: {} {}",
            self.players[player].username,
            command,
            args.join(" ")
        );

        // Handle worldedit commands
        if worldedit::execute_command(self, player, command, &mut args) {
            // If the command was handled, there is no need to continue;
            return false;
        }

        match command {
            "rtps" => {
                if args.is_empty() {
                    let report = self.timings.generate_report();
                    if let Some(report) = report {
                        self.players[player].send_chat_message(&TextComponent::from_legacy_text(
                            &format!(
                            "&6RTPS from last 10s, 1m, 5m, 15m: &a{:.1}, {:.1}, {:.1}, {:.1} ({})",
                            report.ten_s, report.one_m, report.five_m, report.fifteen_m, self.tps
                        ),
                        ));
                    } else {
                        self.players[player].send_chat_message(&TextComponent::from_legacy_text(
                            &format!("&6No timings data. &a({})", self.tps),
                        ));
                    }

                    return false;
                }

                let tps = if let Ok(tps) = args[0].parse::<f32>() {
                    if !tps.is_finite() {
                        self.players[player].send_error_message("RTPS must be a finite number!");
                        return false;
                    }
                    if tps < 0.0 {
                        self.players[player].send_error_message("RTPS cannot be negative!");
                        return false;
                    }
                    Tps::Limited(tps)
                } else if !args[0].is_empty() && "unlimited".starts_with(args[0]) {
                    Tps::Unlimited
                } else {
                    self.players[player].send_error_message("Unable to parse rtps!");
                    return false;
                };

                self.apply_rtps(tps);
                self.players[player].send_system_message("The rtps was successfully set.");
            }
            "radv" | "radvance" => {
                if args.is_empty() {
                    self.players[player]
                        .send_error_message("Please specify a number of ticks to advance.");
                    return false;
                }
                let ticks = if let Ok(ticks) = args[0].parse::<u32>() {
                    ticks
                } else {
                    self.players[player].send_error_message("Unable to parse ticks!");
                    return false;
                };
                let start_time = Instant::now();
                self.advance_ticks(u64::from(ticks));
                self.players[player].send_system_message(&format!(
                    "Plot has been advanced by {} ticks ({:?})",
                    ticks,
                    start_time.elapsed()
                ));
            }
            "tick" => match args.first().copied() {
                None => match self.current_tick() {
                    Some(tick) => {
                        self.players[player].send_system_message(&format!("Tick: {}", tick));
                    }
                    None => self.players[player]
                        .send_error_message("No circuit is compiled, so there is no tick."),
                },
                Some("seq") if args.len() == 1 => match self.start_seq_mode() {
                    Ok(()) => self.players[player].send_system_message(
                        "Reservation mode enabled: scripts disconnected, rtps set to 0.",
                    ),
                    Err(code) => {
                        let message = self.seq_error(code);
                        self.players[player].send_error_message(&message);
                    }
                },
                Some("freeze") if args.len() == 1 => {
                    self.freeze_rtps();
                    self.players[player]
                        .send_system_message("Tick rate frozen (rtps 0).");
                }
                Some("unfreeze") if args.len() == 1 => match self.unfreeze_rtps() {
                    Some(_) => self
                        .players[player]
                        .send_system_message("Tick rate restored."),
                    None => self
                        .players[player]
                        .send_error_message("Tick rate was not frozen."),
                },
                Some("step") => {
                    let Some(step) = args.get(1).and_then(|arg| arg.parse::<u64>().ok()) else {
                        self.players[player]
                            .send_error_message("Usage: /tick step <even number>");
                        return false;
                    };
                    if step == 0 || step % 2 != 0 {
                        self.players[player].send_error_message(
                            "/tick step only accepts even numbers (/tick step 2 = /radv 1).",
                        );
                        return false;
                    }
                    let ticks = step / 2;
                    let start_time = Instant::now();
                    self.advance_ticks(ticks);
                    self.players[player].send_system_message(&format!(
                        "Plot has been advanced by {} ticks ({:?})",
                        ticks,
                        start_time.elapsed()
                    ));
                }
                Some("rate") => {
                    if let Some(arg) = args.get(1) {
                        let tps = if let Ok(tps) = arg.parse::<f32>() {
                            if !tps.is_finite() || tps < 0.0 {
                                self.players[player]
                                    .send_error_message("RTPS must be a finite, non-negative number!");
                                return false;
                            }
                            Tps::Limited(tps)
                        } else if !arg.is_empty() && "unlimited".starts_with(arg) {
                            Tps::Unlimited
                        } else {
                            self.players[player].send_error_message("Unable to parse rtps!");
                            return false;
                        };
                        self.apply_rtps(tps);
                    } else {
                        // Bare `/tick rate` sets 20 (BACKLOG.md BUG-023).
                        self.apply_rtps(Tps::Limited(20.0));
                    }
                    self.players[player]
                        .send_system_message("The rtps was successfully set.");
                }
                Some(_) => self.players[player].send_error_message("Usage: /tick [seq]"),
            },
            "teleport" | "tp" => {
                if args.len() == 3 {
                    let player_pos = self.players[player].pos;
                    let x;
                    let y;
                    let z;
                    if let Ok(x_arg) = parse_relative_coord(args[0], player_pos.x) {
                        x = x_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse x coordinate!");
                        return false;
                    }
                    if let Ok(y_arg) = parse_relative_coord(args[1], player_pos.y) {
                        y = y_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse y coordinate!");
                        return false;
                    }
                    if let Ok(z_arg) = parse_relative_coord(args[2], player_pos.z) {
                        z = z_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse z coordinate!");
                        return false;
                    }
                    self.players[player]
                        .send_system_message(&format!("Teleporting to ({}, {}, {})", x, y, z));
                    self.players[player].teleport(PlayerPos::new(x, y, z));
                } else if args.len() == 1 {
                    self.players[player]
                        .send_system_message(&format!("Teleporting to {}", args[0]));
                    let uuid = self.players[player].uuid;
                    let player = self.leave_plot(uuid);
                    let _ = self
                        .message_sender
                        .send(Message::PlayerTeleportOther(player, args[0].to_string()));
                    return true;
                } else {
                    self.players[player]
                        .send_error_message("Invalid number of arguments for teleport command!");
                }
            }
            "stop" => {
                let _ = self.message_sender.send(Message::Shutdown);
            }
            "plot" | "p" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return false;
                }
                let command = args.remove(0);
                self.handle_plot_command(player, command, &args);
            }
            "redpiler" | "rp" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return false;
                }
                let command = args.remove(0);
                self.handle_redpiler_command(player, command, &args);
            }
            "speed" => {
                if args.len() != 1 {
                    self.players[player].send_error_message("/speed <0-10>");
                    return false;
                }
                if let Ok(speed_arg) = args[0].parse::<f32>() {
                    if speed_arg < 0.0 {
                        self.players[player]
                            .send_error_message("Silly child, you can't have a negative flyspeed!");
                        return false;
                    }
                    if speed_arg > 10.0 {
                        self.players[player].send_error_message(
                            "For performance reasons player speed cannot be higher than 10.",
                        );
                        return false;
                    }
                    if speed_arg.is_nan() {
                        self.players[player]
                            .send_error_message("You can't set your speed to NaN or -NaN.");
                        return false;
                    }
                    self.players[player].fly_speed = speed_arg;
                    self.players[player].update_player_abilities();
                    let username = self.players[player].username.clone();
                    self.players[player].send_system_message(&format!(
                        "Set flying speed to {} for {}",
                        speed_arg, username
                    ));
                } else {
                    self.players[player].send_error_message("Unable to parse speed value");
                }
            }
            "gmsp" => self.change_player_gamemode(player, Gamemode::Spectator),
            "gmc" => self.change_player_gamemode(player, Gamemode::Creative),
            "gamemode" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return false;
                }
                let name = args.remove(0);
                let gamemode = match name {
                    "creative" | "1" => Gamemode::Creative,
                    "spectator" | "3" => Gamemode::Spectator,
                    _ => {
                        self.players[player].send_error_message("Unknown gamemode");
                        return false;
                    }
                };
                self.change_player_gamemode(player, gamemode);
            }
            "container" => {
                if args.len() != 2 {
                    self.players[player].send_error_message("Usage: /container [type] [power]");
                    return false;
                }

                let power = if let Ok(p) = args[1].parse() {
                    p
                } else {
                    self.players[player].send_error_message("Unable to parse power!");
                    return false;
                };

                let container_ty = match args[0].parse() {
                    Ok(ty) => ty,
                    Err(()) => {
                        self.players[player].send_error_message(
                            "Container type must be one of [barrel, furnace, hopper]",
                        );
                        return false;
                    }
                };

                if !(1..=15).contains(&power) {
                    self.players[player].send_error_message(
                        "Container power must be greater than 0 and lower than 15!",
                    );
                    return false;
                }

                let item = ItemStack::container_with_ss(container_ty, power);
                let slot = 36 + self.players[player].selected_slot;
                self.players[player].set_inventory_slot(slot, Some(item));
            }
            "worldsendrate" | "wsr" => {
                if args.is_empty() {
                    self.players[player].send_system_message(&format!(
                        "Current world send rate: {} Hz",
                        self.world.world_send_rate.0
                    ));
                    return false;
                }

                if args.len() != 1 {
                    self.players[player].send_error_message("Usage: /worldsendrate [hertz]");
                    return false;
                }

                let Ok(hertz) = args[0].parse::<f32>() else {
                    self.players[player].send_error_message("Unable to parse send rate!");
                    return false;
                };
                if !hertz.is_finite() {
                    self.players[player]
                        .send_error_message("The world send rate must be a finite number!");
                    return false;
                }
                if hertz < 0.0 {
                    self.players[player]
                        .send_error_message("The world send rate cannot be negative!");
                    return false;
                }
                if hertz > 1000.0 {
                    self.players[player]
                        .send_error_message("The world send rate cannot be higher than 1000!");
                    return false;
                }

                self.world.world_send_rate = WorldSendRate(hertz);
                self.world.pending_sounds.clear();
                self.last_world_send_time = Instant::now();
                self.players[player]
                    .send_system_message("The world send rate was successfully set.");
            }
            "memory" => {
                match args.first().copied() {
                    Some("reload") => match self.reload_memories() {
                        ReloadOutcome::CompiledIoOnly => self.players[player].send_error_message(
                            "/memory reload is not available while a circuit compiled with -i is running.",
                        ),
                        ReloadOutcome::Reloaded { loaded, rejected } => {
                            self.players[player].send_system_message(&format!(
                                "Reloaded {} memory definition(s).",
                                loaded
                            ));
                            // Files that failed to load would otherwise only show
                            // up in the log (BACKLOG.md BUG-021).
                            for message in rejected.iter().take(5) {
                                self.players[player].send_error_message(message);
                            }
                            if rejected.len() > 5 {
                                self.players[player].send_error_message(&format!(
                                    "...and {} more rejected file(s), see the log.",
                                    rejected.len() - 5
                                ));
                            }
                        }
                    },
                    _ => self
                        .players[player]
                        .send_error_message("Usage: /memory reload"),
                }
            }
            "coord" => {
                self.handle_coord_command(player, &args);
            }
            "io" => match args.first().copied() {
                Some("list") => {
                    let summary = self.io_summary();
                    self.players[player].send_system_message(&summary);
                }
                Some("reload") | Some("r") => match self.reload_io() {
                    ReloadOutcome::CompiledIoOnly => self.players[player].send_error_message(
                        "/io reload is not available while a circuit compiled with -i is running.",
                    ),
                    ReloadOutcome::Reloaded { loaded, rejected } => {
                        self.players[player]
                            .send_system_message(&format!("Reloaded {} IO group(s).", loaded));
                        for message in rejected.iter().take(5) {
                            self.players[player].send_error_message(message);
                        }
                        if rejected.len() > 5 {
                            self.players[player].send_error_message(&format!(
                                "...and {} more rejected line(s), see the log.",
                                rejected.len() - 5
                            ));
                        }
                    }
                },
                _ => self
                    .players[player]
                    .send_error_message("Usage: /io <list|reload>"),
            },
            "version" => {
                self.players[player].send_system_message(&get_version_string());
            }
            _ => self.players[player].send_error_message("Command not found!"),
        }
        false
    }
}

bitflags! {
    pub struct CommandFlags: u32 {
        const ROOT = 0x0;
        const LITERAL = 0x1;
        const ARGUMENT = 0x2;
        const EXECUTABLE = 0x4;
        const REDIRECT = 0x8;
        const HAS_SUGGESTIONS_TYPE = 0x10;
    }
}

// In the future a DSL or some type of generation would be much better.
// For more information, see https://wiki.vg/Command_Data
/// The `DeclareCommands` packet that is sent when the player joins.
/// This is used for command autocomplete.
pub static DECLARE_COMMANDS: LazyLock<PacketEncoder> = LazyLock::new(|| {
    let mut packet = CCommands {
        nodes: vec![
            // 0: Root Node
            Node {
                flags: CommandFlags::ROOT.bits() as i8,
                children: vec![
                    1, 4, 5, 6, 8, 10, 11, 13, 18, 34, 41, 43, 44, 45, 49, 51, 52, 53, 55, 56,
                    57, 59, 61, 81,
                ],
                redirect_node: None,
                name: None,
                parser: None,
                suggestions_type: None,
            },
            // 1: /teleport
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![2, 3],
                redirect_node: None,
                name: Some("teleport"),
                parser: None,
                suggestions_type: None,
            },
            // 2: /teleport [x, y, z]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("x, y, z"),
                parser: Some(Parser::Vec3),
                suggestions_type: None,
            },
            // 3: /teleport [player]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("player"),
                parser: Some(Parser::Entity(3)), // Only allow one player
                suggestions_type: None,
            },
            // 4: /tp
            Node {
                flags: (CommandFlags::REDIRECT | CommandFlags::LITERAL).bits() as i8,
                children: vec![],
                redirect_node: Some(1),
                name: Some("tp"),
                parser: None,
                suggestions_type: None,
            },
            // 5: /stop
            Node {
                flags: (CommandFlags::EXECUTABLE | CommandFlags::LITERAL).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("stop"),
                parser: None,
                suggestions_type: None,
            },
            // 6: /rtps
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![7],
                redirect_node: None,
                name: Some("rtps"),
                parser: None,
                suggestions_type: None,
            },
            // 7: /rtps [rtps]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("rtps"),
                parser: Some(Parser::Float(0.0, f32::MAX)),
                suggestions_type: None,
            },
            // 8: /radvance
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![9],
                redirect_node: None,
                name: Some("radvance"),
                parser: None,
                suggestions_type: None,
            },
            // 9: /radvance [rticks]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("rticks"),
                parser: Some(Parser::Integer(0, i32::MAX)),
                suggestions_type: None,
            },
            // 10: /radv
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(9),
                name: Some("radv"),
                parser: None,
                suggestions_type: None,
            },
            // 11: /speed
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![12],
                redirect_node: None,
                name: Some("speed"),
                parser: None,
                suggestions_type: None,
            },
            // 12: /speed [speed]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("speed"),
                parser: Some(Parser::Float(0.0, 10.0)),
                suggestions_type: None,
            },
            // 13: /plot
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![14, 15, 16, 17, 19, 20, 21, 22, 24, 25, 27, 28, 29, 39, 40],
                redirect_node: None,
                name: Some("plot"),
                parser: None,
                suggestions_type: None,
            },
            // 14: /plot info
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("info"),
                parser: None,
                suggestions_type: None,
            },
            // 15: /plot i
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(14),
                name: Some("i"),
                parser: None,
                suggestions_type: None,
            },
            // 16: /plot claim
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("claim"),
                parser: None,
                suggestions_type: None,
            },
            // 17: /plot c
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(9),
                name: Some("c"),
                parser: None,
                suggestions_type: None,
            },
            // 18: /p
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(13),
                name: Some("p"),
                parser: None,
                suggestions_type: None,
            },
            // 19: /p auto
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("auto"),
                parser: None,
                suggestions_type: None,
            },
            // 20: /p a
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(19),
                name: Some("a"),
                parser: None,
                suggestions_type: None,
            },
            // 21: /p middle
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("middle"),
                parser: None,
                suggestions_type: None,
            },
            // 22: /p visit
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![23],
                redirect_node: None,
                name: Some("visit"),
                parser: None,
                suggestions_type: None,
            },
            // 23: /p visit [player]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("player"),
                parser: Some(Parser::Entity(3)),
                suggestions_type: None,
            },
            // 24: /p v
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(22),
                name: Some("v"),
                parser: None,
                suggestions_type: None,
            },
            // 25: /p teleport
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![26],
                redirect_node: None,
                name: Some("teleport"),
                parser: None,
                suggestions_type: None,
            },
            // 26: /p teleport [x, z]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("x, z"),
                parser: Some(Parser::Vec2),
                suggestions_type: None,
            },
            // 27: /p tp
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(25),
                name: Some("tp"),
                parser: None,
                suggestions_type: None,
            },
            // 28: /p select
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("select"),
                parser: None,
                suggestions_type: None,
            },
            // 29: /p sel
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(28),
                name: Some("sel"),
                parser: None,
                suggestions_type: None,
            },
            // 30-33: /whitelist —— 白名单功能已整体删除（见 BACKLOG.md BUG-005）。
            // 这四个节点保留在数组中是为了避免整体重编号（甲方案）；根节点的
            // children 已不再引用节点 30，因此客户端看不到、也补全不出它们。
            // 30: /whitelist
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![31, 32],
                redirect_node: None,
                name: Some("whitelist"),
                parser: None,
                suggestions_type: None,
            },
            // 31: /whitelist add
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![33],
                redirect_node: None,
                name: Some("add"),
                parser: None,
                suggestions_type: None,
            },
            // 32: /whitelist remove
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![33],
                redirect_node: None,
                name: Some("remove"),
                parser: None,
                suggestions_type: None,
            },
            // 33: /whitelist add|remove [username]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("username"),
                parser: Some(Parser::Entity(3)),
                suggestions_type: None,
            },
            // 34: /container
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![35, 36, 37],
                redirect_node: None,
                name: Some("container"),
                parser: None,
                suggestions_type: None,
            },
            // 35: /container barrel
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![38],
                redirect_node: None,
                name: Some("barrel"),
                parser: None,
                suggestions_type: None,
            },
            // 36: /container hopper
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![38],
                redirect_node: None,
                name: Some("hopper"),
                parser: None,
                suggestions_type: None,
            },
            // 37: /container furnace
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![38],
                redirect_node: None,
                name: Some("furnace"),
                parser: None,
                suggestions_type: None,
            },
            // 38: /container [type] [power]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("power"),
                parser: Some(Parser::Integer(0, 15)),
                suggestions_type: None,
            },
            // 39: /plot lock
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("lock"),
                parser: None,
                suggestions_type: None,
            },
            // 40: /plot unlock
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("unlock"),
                parser: None,
                suggestions_type: None,
            },
            // 41: //load
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![42],
                redirect_node: None,
                name: Some("/load"),
                parser: None,
                suggestions_type: None,
            },
            // 42: //load [filename]
            Node {
                flags: (CommandFlags::ARGUMENT
                    | CommandFlags::EXECUTABLE
                    | CommandFlags::HAS_SUGGESTIONS_TYPE)
                    .bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("filename"),
                parser: Some(Parser::String(0)),
                suggestions_type: Some("minecraft:ask_server"),
            },
            // 43: /toggleautorp
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("toggleautorp"),
                parser: None,
                suggestions_type: None,
            },
            // 44: /redpiler
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![46, 47, 48], // Children are compile, inspect, reset
                redirect_node: None,
                name: Some("redpiler"),
                parser: None,
                suggestions_type: None,
            },
            // 45: /rp
            Node {
                flags: (CommandFlags::REDIRECT | CommandFlags::LITERAL).bits() as i8,
                children: vec![],
                redirect_node: Some(44), // Redirect to /redpiler
                name: Some("rp"),
                parser: None,
                suggestions_type: None,
            },
            // 46: /redpiler compile
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78],
                redirect_node: None,
                name: Some("compile"),
                parser: None,
                suggestions_type: None,
            },
            // 47: /redpiler inspect
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("inspect"),
                parser: None,
                suggestions_type: None,
            },
            // 48: /redpiler reset
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("reset"),
                parser: None,
                suggestions_type: None,
            },
            // 49: /worldsendrate
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![50],
                redirect_node: None,
                name: Some("worldsendrate"),
                parser: None,
                suggestions_type: None,
            },
            // 50: /worldsendrate [hertz]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("hertz"),
                parser: Some(Parser::Float(0.0, 1000.0)),
                suggestions_type: None,
            },
            // 51: /wsr
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(49),
                name: Some("wsr"),
                parser: None,
                suggestions_type: None,
            },
            // 52: /version
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("version"),
                parser: None,
                suggestions_type: None,
            },
            // 53: /gamemode
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![54],
                redirect_node: None,
                name: Some("gamemode"),
                parser: None,
                suggestions_type: None,
            },
            // 54: /gamemode [gamemode]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("gamemode"),
                parser: Some(Parser::GameMode),
                suggestions_type: None,
            },
            // 55: /gmc
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("gmc"),
                parser: None,
                suggestions_type: None,
            },
            // 56: /gmsp
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("gmsp"),
                parser: None,
                suggestions_type: None,
            },
            // 57: /memory
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![58],
                redirect_node: None,
                name: Some("memory"),
                parser: None,
                suggestions_type: None,
            },
            // 58: /memory reload
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("reload"),
                parser: None,
                suggestions_type: None,
            },
            // 59: /tick
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![60, 62, 63, 64, 66],
                redirect_node: None,
                name: Some("tick"),
                parser: None,
                suggestions_type: None,
            },
            // 60: /tick seq
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("seq"),
                parser: None,
                suggestions_type: None,
            },
            // 61: /io
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![79, 80],
                redirect_node: None,
                name: Some("io"),
                parser: None,
                suggestions_type: None,
            },
            // 62: /tick freeze
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("freeze"),
                parser: None,
                suggestions_type: None,
            },
            // 63: /tick unfreeze
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("unfreeze"),
                parser: None,
                suggestions_type: None,
            },
            // 64: /tick step
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![65],
                redirect_node: None,
                name: Some("step"),
                parser: None,
                suggestions_type: None,
            },
            // 65: /tick step [even ticks]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("even"),
                parser: Some(Parser::Integer(2, i32::MAX)),
                suggestions_type: None,
            },
            // 66: /tick rate
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![67],
                redirect_node: None,
                name: Some("rate"),
                parser: None,
                suggestions_type: None,
            },
            // 67: /tick rate [rtps]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("rtps"),
                parser: Some(Parser::Float(0.0, 1000.0)),
                suggestions_type: None,
            },
            // 68: /redpiler compile --optimize
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--optimize"),
                parser: None,
                suggestions_type: None,
            },
            // 69: /redpiler compile --export
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--export"),
                parser: None,
                suggestions_type: None,
            },
            // 70: /redpiler compile --io-only
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--io-only"),
                parser: None,
                suggestions_type: None,
            },
            // 71: /redpiler compile --update
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--update"),
                parser: None,
                suggestions_type: None,
            },
            // 72: /redpiler compile --export-dot
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--export-dot"),
                parser: None,
                suggestions_type: None,
            },
            // 73: /redpiler compile --wire-dot-out
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--wire-dot-out"),
                parser: None,
                suggestions_type: None,
            },
            // 74: /redpiler compile --illegal-states-out
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--illegal-states-out"),
                parser: None,
                suggestions_type: None,
            },
            // 75: /redpiler compile --wire-cross-out
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--wire-cross-out"),
                parser: None,
                suggestions_type: None,
            },
            // 76: /redpiler compile --print-after-all
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--print-after-all"),
                parser: None,
                suggestions_type: None,
            },
            // 77: /redpiler compile --print-before-backend
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--print-before-backend"),
                parser: None,
                suggestions_type: None,
            },
            // 78: /redpiler compile --passes=
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("--passes="),
                parser: None,
                suggestions_type: None,
            },
            // 79: /io list
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("list"),
                parser: None,
                suggestions_type: None,
            },
            // 80: /io reload
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("reload"),
                parser: None,
                suggestions_type: None,
            },
            // 81: /coord
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![82, 83, 84, 88, 89],
                redirect_node: None,
                name: Some("coord"),
                parser: None,
                suggestions_type: None,
            },
            // 82: /coord in
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![85, 86],
                redirect_node: None,
                name: Some("in"),
                parser: None,
                suggestions_type: None,
            },
            // 83: /coord new
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![87],
                redirect_node: None,
                name: Some("new"),
                parser: None,
                suggestions_type: None,
            },
            // 84: /coord add
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![87],
                redirect_node: None,
                name: Some("add"),
                parser: None,
                suggestions_type: None,
            },
            // 85: /coord in [file]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("file"),
                parser: Some(Parser::String(0)),
                suggestions_type: None,
            },
            // 86: /coord in io.txt [name]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("io name"),
                parser: Some(Parser::String(0)),
                suggestions_type: None,
            },
            // 87: /coord new|add [type]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("type"),
                parser: Some(Parser::String(0)),
                suggestions_type: None,
            },
            // 88: /coord undo
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("undo"),
                parser: None,
                suggestions_type: None,
            },
            // 89: /coord redo
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("redo"),
                parser: None,
                suggestions_type: None,
            },
        ],
        root_index: 0,
    };
    super::worldedit::add_command_completions(&mut packet.nodes);
    packet.encode()
});
