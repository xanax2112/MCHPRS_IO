use crate::permissions::PermissionsConfig;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::{fs, sync::LazyLock};
use toml_edit::{value, DocumentMut};

pub static CONFIG: LazyLock<ServerConfig> = LazyLock::new(|| ServerConfig::load("Config.toml"));

/// How often the automation bridge samples its data ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoMode {
    /// Sample every tick, so no state change is ever merged or dropped.
    Exact,
    /// Sample once per tick batch (faster on accelerated plots, but automation
    /// reacts later).
    Performance,
}

trait ConfigSerializeDefault {
    fn fix_config(self, name: &str, doc: &mut DocumentMut);
}

macro_rules! impl_simple_default {
    ( $( $type:ty ),* ) => {
        $(
            impl ConfigSerializeDefault for $type {
                fn fix_config(self, name: &str, doc: &mut DocumentMut) {
                    doc.entry(name).or_insert_with(|| value(self));
                }
            }
        )*
    }
}

impl_simple_default!(String, i64, bool);

impl<T> ConfigSerializeDefault for Option<T> {
    fn fix_config(self, _: &str, _: &mut DocumentMut) {
        assert!(self.is_none(), "`Some` as default is unimplemented");
    }
}

macro_rules! gen_config {
    (
        $( $( #[$meta:meta] )* $name:ident: $type:ty = $default:expr),*
    ) => {
        #[derive(Serialize, Deserialize)]
        pub struct ServerConfig {
            $(
                $( #[$meta] )*
                pub $name: $type,
            )*
        }

        impl ServerConfig {
            fn load(config_file: &str) -> ServerConfig {
                let str = fs::read_to_string("Config.toml").unwrap_or_default();
                let mut doc = str.parse::<DocumentMut>().unwrap();

                // 0.4.x keys are rewritten in place so an old `Config.toml`
                // keeps working without the user editing anything.
                if let Some(old) = doc.remove("automation_port") {
                    if !doc.contains_key("io_port") {
                        doc["io_port"] = old;
                    }
                }
                for legacy in [
                    "automation_data_port_events",
                    "automation_per_tick_push",
                    "auto_redpiler",
                    // 0.5.2-alpha: `mode` became the boolean `performance`; the
                    // old value is ignored and the line is dropped (BACKLOG.md
                    // BUG-001).
                    "mode",
                ] {
                    doc.remove(legacy);
                }

                $(
                    <$type as ConfigSerializeDefault>::fix_config($default, stringify!($name), &mut doc);
                )*

                let patched = doc.to_string();
                if str != patched {
                    let mut file = fs::OpenOptions::new().create(true).write(true).open(&config_file).unwrap();
                    write!(file, "{}", patched).unwrap();
                }

                toml::from_str(&patched).unwrap()
            }

            /// The sampling mode from `Config.toml`: `performance = true`
            /// samples once per tick batch, `false` (the default) every tick.
            pub fn io_mode(&self) -> IoMode {
                if self.performance {
                    IoMode::Performance
                } else {
                    IoMode::Exact
                }
            }
        }
    };
}

gen_config! {
    bind_address: String = "0.0.0.0:25565".to_string(),
    motd: String = "Minecraft High Performance Redstone Server".to_string(),
    chat_format: String = "<{username}> {message}".to_string(),
    max_players: i64 = 99999,
    view_distance: i64 = 8,
    schemati: bool = false,
    luckperms: Option<PermissionsConfig> = None,
    block_in_hitbox: bool = true,
    velocity: Option<VelocityConfig> = None,
    /// TCP port of the automation bridge. The listener is bound to 127.0.0.1.
    io_port: i64 = 25585,
    /// `false` samples every tick (exact); `true` samples once per tick batch.
    performance: bool = false
}

#[derive(Serialize, Deserialize)]
pub struct VelocityConfig {
    pub enabled: bool,
    pub secret: String,
}
