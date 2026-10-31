//! Bedrock 入站包 → Java 入站包的转义。
//!
//! 目的（BACKLOG.md BUG-032「玩家化 · 入站转义」）：让 `plot` 的
//! `handle_packets_for_player` 完全复用现有 Java 逻辑 —— 它只认
//! `Box<dyn ServerBoundPacket>`，所以只要把 Bedrock 的输入转成 Java 包，
//! 命令、方块交互、移动、潜行等等都不需要为 Bedrock 再写一套。
//!
//! **坐标换算**：Bedrock 的 `y` 比 MCHPRS 少 64，所以转回去要 `y + 64`；x/z 相同。
//!
//! 只转义真正有语义的包；`SubChunkRequest` 这类 Bedrock 专有的直接丢弃。

use crate::packets::serverbound::{
    SChatCommand, SChatMessage, SPlayerAction, SSetPlayerPositionAndRotation, ServerBoundPacket,
};
use bedrock::protocol::V766 as Protocol;

/// `PlayerAuthInputFlags::VerticalCollision`。
///
/// 不直接 import 这个枚举：它在 vendored crate 里位于私有子模块，只通过 `pub use` 重导出，
/// 路径随时会随上游重构变化；这里钉住数值（1 << 50），并在注释里留下出处。
const VERTICAL_COLLISION: u128 = 1 << 50;

/// Java 版聊天的长度上限。Geyser `MessageTranslator.isTooLong` 用的也是 256，
/// 超过就直接丢掉（Geyser 会额外给玩家发一句"太长"，这里没做，见 BACKLOG 说明）。
const MAX_CHAT_LENGTH: usize = 256;

/// 把一条 Bedrock 包转义成 0..n 条 Java 入站包。
pub fn translate(packet: &Protocol) -> Vec<Box<dyn ServerBoundPacket>> {
    let mut out: Vec<Box<dyn ServerBoundPacket>> = Vec::new();
    match packet {
        Protocol::PlayerAuthInputPacket(input) => {
            // **Bedrock 的旋转元组是 `(pitch, yaw)`**（Geyser `Entity#getBedrockRotation`
            // 返回 `Vector3f(pitch, yaw, headYaw)`），Java 侧要的是分别的 yaw/pitch。
            let (pitch, yaw) = input.player_rotation;

            // Bedrock 没有直接的 on_ground 字段：用「垂直碰撞位 + 未向上运动」近似，
            // 这与客户端自己的判定基本一致。
            let vertical_collision = input.input_data & VERTICAL_COLLISION != 0;
            let on_ground = vertical_collision && input.velocity.1 <= 0.0;

            out.push(Box::new(SSetPlayerPositionAndRotation {
                x: input.player_position.0 as f64,
                y: input.player_position.1 as f64 + 64.0,
                z: input.player_position.2 as f64,
                yaw,
                pitch,
                on_ground,
            }));

            // 方块交互：Bedrock 的 PlayerBlockActionData → Java 的 SPlayerAction。
            if let Some(actions) = &input.player_block_actions {
                for action in actions {
                    // `BlockPos` 只 derive 了 `Clone, Debug`（不是 `Copy`），所以这里必须借，
                    // 否则就是从共享借用里 move 出去（E0507）。
                    let Some(pos) = &action.position else { continue };
                    // 只转「开始挖掘」：plot 的 handle_player_action 只对 status == 0 生效，
                    // 其余 status 会掉进背包分支，没有意义也白造包。
                    //
                    // 创造模式必须同时认 `CreativeDestroyBlock`：创造是瞬时破坏，客户端报的是
                    // 13 而不是 0（BACKLOG.md BUG-032 复查第 15 条）。
                    if !matches!(
                        action.action_type,
                        bedrock::protocol::v662::enums::PlayerActionType::StartDestroyBlock
                            | bedrock::protocol::v662::enums::PlayerActionType::CreativeDestroyBlock
                    ) {
                        continue;
                    }
                    out.push(Box::new(SPlayerAction {
                        status: 0,
                        x: pos.x,
                        y: pos.y + 64,
                        z: pos.z,
                        face: action.facing.unwrap_or(0) as i8,
                        sequence: 0,
                    }));
                }
            }
        }
        Protocol::TextPacket(text) => {
            // 玩家在聊天框里打字（或把命令当聊天发出来）。
            //
            // 语义照抄 Geyser `BedrockTextTranslator`（core/src/main/java/org/geysermc/geyser/
            // translator/protocol/bedrock/BedrockTextTranslator.java）：
            //   1. 归一化空白（连续空白压成一个空格、去掉首尾）—— 原版客户端对聊天/命令都这么做；
            //   2. 空白 → 直接丢弃（Java 版也不转）；
            //   3. 以 `/` 开头 → 当命令（Java 的 `SChatCommand` 不带前导 `/`）；
            //   4. 超过 256 → 丢弃。
            // 注意 Geyser 这里**不看** `TextPacket` 的 type 字段，直接取 message 字段。
            let raw = text_message(&text.message_type);
            let message = normalize_space(raw);
            if message.is_empty() {
                return out;
            }
            if let Some(command) = message.strip_prefix('/') {
                out.push(command_packet(command));
            } else if message.encode_utf16().count() <= MAX_CHAT_LENGTH {
                out.push(Box::new(SChatMessage {
                    message,
                    // 1.20.4 的 `SChatMessage` 带签名/时间戳字段；Bedrock 端没有对应物，
                    // 用"无签名 + 零时间戳"的离线写法，MCHPRS 的 `handle_chat_message` 只用 message。
                    timestamp: 0,
                    salt: 0,
                    signature: None,
                    message_count: 0,
                    acknowledged: [0; 3],
                }));
            }
        }
        Protocol::CommandRequestPacket(request) => {
            // `internal = true` 是命令方块/内部来源触发，不是玩家输入，别当命令执行。
            if request.internal {
                return out;
            }
            // Bedrock 的命令带前导 `/`，Java 的 `SChatCommand` 不带。
            let command = request.command.trim_start_matches('/');
            if !command.is_empty() {
                out.push(command_packet(command));
            }
        }
        _ => {}
    }
    out
}

/// 构造一条 Java `SChatCommand`（`command` **不带**前导 `/`）。
fn command_packet(command: &str) -> Box<dyn ServerBoundPacket> {
    Box::new(SChatCommand {
        command: command.to_string(),
        timestamp: 0,
        salt: 0,
        argument_signatures: Vec::new(),
        message_count: 0,
        acknowledged: [0; 3],
    })
}

/// 从 `TextPacket` 的 type 里取出聊天正文。
///
/// Geyser 取的是 Cloudburst `TextPacket#getMessage()`，它对所有 type 都是同一个字段；
/// 我们这边的 vendored 版本把它拆成了枚举，所以按变体分别取正文字段。
/// 客户端上行只会发 `Chat`/`Whisper`/`Raw`，其余变体只是兜底。
fn text_message(message_type: &bedrock::protocol::v662::enums::TextPacketType) -> &str {
    use bedrock::protocol::v662::enums::TextPacketType;
    match message_type {
        TextPacketType::Chat { message, .. }
        | TextPacketType::Whisper { message, .. }
        | TextPacketType::Announcement { message, .. }
        | TextPacketType::Translate { message, .. }
        | TextPacketType::Popup { message, .. }
        | TextPacketType::JukeboxPopup { message, .. } => message.as_str(),
        TextPacketType::Raw(text)
        | TextPacketType::Tip(text)
        | TextPacketType::SystemMessage(text)
        | TextPacketType::TextObjectWhisper(text)
        | TextPacketType::TextObject(text)
        | TextPacketType::TextObjectAnnouncement(text) => text.as_str(),
    }
}

/// 归一化空白：连续空白压成一个空格、去掉首尾。
///
/// 逐行照抄 Geyser `MessageTranslator.normalizeSpace`：
/// `Character.isWhitespace` 不认不换行空格（U+00A0），它被当成普通字符替换成普通空格，
/// 因此它会**打断**一段空白（`"a\u{00A0} b"` → `"a  b"`）。
fn normalize_space(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    let mut started = false;
    for ch in input.chars() {
        if is_java_whitespace(ch) {
            if started {
                pending_space = true;
            }
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            started = true;
            out.push(if ch == '\u{00A0}' { ' ' } else { ch });
        }
    }
    out
}

/// 与 Java `Character.isWhitespace` 对齐的空白判定。
///
/// Rust 的 `char::is_whitespace` 用的是 Unicode `White_Space` 属性，**包含**不换行空格；
/// Java 的那套**排除**它们（`\u00A0`、`\u2007`、`\u202F`）。Geyser 依赖"不换行空格不是空白"
/// 这个前提：它会被当成普通字符替换成普通空格，从而**打断**一段空白。
fn is_java_whitespace(ch: char) -> bool {
    ch.is_whitespace() && !matches!(ch, '\u{00A0}' | '\u{2007}' | '\u{202F}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::serverbound::ServerBoundPacketHandler;
    use bedrock::protocol::v662::enums::TextPacketType;
    use bedrock::protocol::v685::packets::TextPacket;

    /// 把转义出来的包真正喂给一个 handler（和 core 的消费方式一致），把结果抓出来。
    #[derive(Default)]
    struct Capture {
        commands: Vec<String>,
        messages: Vec<String>,
    }

    impl ServerBoundPacketHandler for Capture {
        fn handle_chat_command(&mut self, packet: SChatCommand, _player_idx: usize) {
            self.commands.push(packet.command);
        }

        fn handle_chat_message(&mut self, packet: SChatMessage, _player_idx: usize) {
            self.messages.push(packet.message);
        }
    }

    fn run(packets: Vec<Box<dyn ServerBoundPacket>>) -> Capture {
        let mut capture = Capture::default();
        for packet in packets {
            packet.handle(&mut capture, 0);
        }
        capture
    }

    fn chat(text: &str) -> Protocol {
        Protocol::TextPacket(Box::new(TextPacket::<Protocol> {
            message_type: TextPacketType::Chat {
                player_name: String::from("tester"),
                message: text.to_string(),
            },
            localize: false,
            sender_xuid: String::new(),
            platform_id: String::new(),
            // 1.21 起末尾多了"过滤后文本"字段（`text.rs` 的出站包也写了同样的事）
            filtered_message: String::new(),
        }))
    }

    /// 空白归一化：行为逐条对着 Geyser `MessageTranslator.normalizeSpace` 抄的。
    #[test]
    fn normalize_space_matches_geyser() {
        assert_eq!(normalize_space("  hello   world  "), "hello world");
        assert_eq!(normalize_space("a\t\tb\nc"), "a b c");
        assert_eq!(normalize_space("   "), "");
        assert_eq!(normalize_space(""), "");
        // 不换行空格不是空白：它变普通空格，并且打断空白（所以这里是两个空格）
        assert_eq!(normalize_space("a\u{00A0} b"), "a  b");
    }

    /// 普通聊天 → `SChatMessage`（正文已归一化）。
    #[test]
    fn chat_text_becomes_chat_message() {
        let capture = run(translate(&chat("  你好   世界  ")));
        assert!(capture.commands.is_empty());
        assert_eq!(capture.messages, vec![String::from("你好 世界")]);
    }

    /// 以 `/` 开头 → 当命令，且去掉前导 `/`（Geyser `BedrockTextTranslator` 的分支）。
    #[test]
    fn slash_text_becomes_command() {
        let capture = run(translate(&chat("/tp   150 20 150")));
        assert!(capture.messages.is_empty());
        assert_eq!(capture.commands, vec![String::from("tp 150 20 150")]);
    }

    /// 全空白 / 超过 256 个 UTF-16 单元 → 丢弃（Geyser 也是直接 return）。
    #[test]
    fn blank_and_too_long_are_dropped() {
        assert!(run(translate(&chat("    "))).messages.is_empty());
        let long = "a".repeat(MAX_CHAT_LENGTH + 1);
        assert!(run(translate(&chat(&long))).messages.is_empty());
        let ok = "a".repeat(MAX_CHAT_LENGTH);
        assert_eq!(run(translate(&chat(&ok))).messages.len(), 1);
    }
}
