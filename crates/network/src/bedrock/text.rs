//! Bedrock 聊天/系统文本编码（R4 的"回执"那一半）。
//!
//! Java 侧发的是 `CSystemChatMessage`（JSON 组件）；Bedrock 对应 `TextPacket`，
//! 类型用 `Raw`（服务端系统消息，直接显示在聊天框里），文本里的 `§` 颜色码会被客户端解析。
//!
//! 玩家聊天（`<player> 消息`）也用 `Raw`：Java 侧的 `chat_format` 已经在服务端拼好了整行，
//! 再用 Bedrock 的 `Chat { player_name, message }` 反而会被客户端再拼一次发出者名字。

use bedrock::protocol::v662::enums::TextPacketType;
use bedrock::protocol::v685::packets::TextPacket;

use crate::bedrock::protocol::Protocol;

/// 构造一行系统聊天文本（`text` 可带 `§` 颜色码）。
pub fn raw_text(text: &str) -> TextPacket<Protocol> {
    TextPacket::<Protocol> {
        message_type: TextPacketType::Raw(text.to_string()),
        localize: false,
        sender_xuid: String::new(),
        platform_id: String::new(),
        // 1.21 起 TextPacket 末尾多了一个"过滤后文本"字段，不发就是空串。
        filtered_message: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedrock::protocol::ProtoCodec;

    fn hex(pkt: &impl ProtoCodec) -> String {
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        buf.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// 期望字节来自 `refs/ProbeText.java`（Cloudburst `Bedrock_v766.CODEC`）：
    /// `00`(RAW) `00`(needsTranslation) `08`+"§eHello" `00`(xuid) `00`(platform) `00`(filtered)。
    #[test]
    fn raw_text_matches_geyser_codec() {
        assert_eq!(
            hex(&raw_text("§eHello")),
            "000008c2a76548656c6c6f000000"
        );
    }
}
