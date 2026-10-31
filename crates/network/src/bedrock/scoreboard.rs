//! Bedrock 计分板（sidebar）编码，对应 Java 版右侧的 redpiler 状态栏（BACKLOG.md BUG-032 R3）。
//!
//! Java 的做法：一个 objective（`redpiler_status`）+ display slot 1（sidebar）+
//! 每行一个 fake player（`redpiler_state`、`flags`、`flag_o`…）的分数，分数用于排序，
//! 数字本身用 `number_format = Blank` 隐藏。
//!
//! Bedrock 的等价物：
//! * `SetDisplayObjective`：`display_slot_name = "sidebar"`、`criteria_name = "dummy"`、
//!   `sort_order` 决定上下顺序（**注意它按 zigzag 编码**：`Descending(1)` 落盘是 `02`）。
//! * `SetScorePacket::Change`：每个 fake player 一个 `ScoreboardId` + 显示文本。
//!
//! **已知平台差异（无法消除）**：Bedrock 的计分板**没有** `number_format: Blank` 的等价物，
//! 右侧分数一定会显示出来。所以这里只保证"行内容与顺序和 Java 一致"，数字必然可见。

use bedrock::protocol::v662::enums::{IdentityDefinitionType, ObjectiveSortOrder};
use bedrock::protocol::v662::packets::{
    ScorePacketInfoChangeEntry, ScorePacketInfoRemoveEntry, SetDisplayObjectivePacket, SetScorePacket,
};
use bedrock::protocol::v662::types::ScoreboardId;

use crate::bedrock::bridge::BedrockSidebar;
use crate::bedrock::protocol::Protocol;

/// Bedrock 侧 objective 的名字。
pub const OBJECTIVE_NAME: &str = "mchprs_status";

/// 计分板显示位置：`sidebar`（右侧）。
const DISPLAY_SLOT: &str = "sidebar";
/// 只做展示，无判据。
const CRITERIA: &str = "dummy";

/// 构造 `SetDisplayObjective`（只在首次下发或标题变化时发）。
pub fn set_display_objective(title: &str) -> SetDisplayObjectivePacket<Protocol> {
    SetDisplayObjectivePacket::<Protocol> {
        display_slot_name: DISPLAY_SLOT.to_string(),
        objective_name: OBJECTIVE_NAME.to_string(),
        objective_display_name: title.to_string(),
        criteria_name: CRITERIA.to_string(),
        // Java 的 sidebar 按分数降序排（分数高的在上）；我们的分数是 `len - i`。
        sort_order: ObjectiveSortOrder::Descending,
    }
}

/// 构造 `SetScorePacket::Change`：每行一条 fake-player 记录，`scoreboard_id` 用行号。
pub fn change_scores(sidebar: &BedrockSidebar) -> SetScorePacket<Protocol> {
    let changes = sidebar
        .lines
        .iter()
        .enumerate()
        .map(|(index, line)| ScorePacketInfoChangeEntry::<Protocol> {
            id: ScoreboardId {
                id: index as i64,
            },
            objective_name: OBJECTIVE_NAME.to_string(),
            score_value: line.score,
            identity_definition_type: IdentityDefinitionType::FakePlayer {
                fake_player_name: line.text.clone(),
            },
        })
        .collect();
    SetScorePacket::Change(changes)
}

/// 构造 `SetScorePacket::Remove`：删掉行号 `[from, to)`（行数变少时用）。
pub fn remove_scores(from: usize, to: usize) -> SetScorePacket<Protocol> {
    let removes = (from..to)
        .map(|index| ScorePacketInfoRemoveEntry::<Protocol> {
            id: ScoreboardId {
                id: index as i64,
            },
            objective_name: OBJECTIVE_NAME.to_string(),
            score_value: 0,
        })
        .collect();
    SetScorePacket::Remove(removes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bedrock::bridge::BedrockSidebarLine;
    use bedrock::protocol::ProtoCodec;

    fn hex(pkt: &impl ProtoCodec) -> String {
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        buf.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn sidebar() -> BedrockSidebar {
        BedrockSidebar {
            title: "§cRedpiler Status".to_string(),
            lines: vec![
                BedrockSidebarLine {
                    name: "redpiler_state".to_string(),
                    score: 2,
                    text: "§a§lRunning".to_string(),
                },
                BedrockSidebarLine {
                    name: "flags".to_string(),
                    score: 1,
                    text: "§7Flags:".to_string(),
                },
            ],
        }
    }

    /// 期望字节来自 `refs/ProbeScoreboard.java`（Cloudburst `Bedrock_v766.CODEC`）。
    #[test]
    fn display_objective_matches_geyser_codec() {
        assert_eq!(
            hex(&set_display_objective("§cRedpiler Status")),
            "07736964656261720d6d63687072735f73746174757312c2a76352656470696c6572205374617475730564756d6d7902"
        );
    }

    #[test]
    fn change_scores_matches_geyser_codec() {
        assert_eq!(
            hex(&change_scores(&sidebar())),
            "0002000d6d63687072735f73746174757302000000030dc2a761c2a76c52756e6e696e67020d6d63687072735f737461747573010000000309c2a737466c6167733a"
        );
    }

    #[test]
    fn remove_scores_matches_geyser_codec() {
        assert_eq!(
            hex(&remove_scores(2, 3)),
            "0101040d6d63687072735f73746174757300000000"
        );
    }
}
