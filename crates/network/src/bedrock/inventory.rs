//! Bedrock 背包容器：打开 / 内容 / 关闭确认，对应 BUG-032 R1。
//!
//! **为什么必须由服务端发这些包**：Bedrock 客户端的背包界面不是"本地按 E 就弹出来"的。
//! 客户端按 E 只发一个 `Interact(action=OpenInventory)`，真正的界面要等服务端回
//! `ContainerOpen`；不回，客户端就一直等着 —— 表现就是"按 E 没反应、背包打不开"。
//!
//! 参考实现 `Geyser/.../PlayerInventoryTranslator`：
//!   `openInventory`  → `ContainerOpenPacket(id=0, type=INVENTORY, uniqueEntityId=-1, pos=脚下)`
//!   `updateInventory`→ `InventoryContentPacket(INVENTORY,36 空)` + `(ARMOR,4 空)` + `(OFFHAND,1 空)`
//!   `closeInventory` → 客户端发 `ContainerClose`，服务端要**原样回一份**（"Client wants close confirmation"）
//!
//! 逐字节期望值来自 `refs/ProbeInventory.java`（Cloudburst `Bedrock_v766.CODEC`）。

use bedrock::protocol::v662::enums::{ContainerID, ContainerType};
use bedrock::protocol::v662::packets::ContainerOpenPacket;
use bedrock::protocol::v662::types::{
    ActorUniqueID, NetworkBlockPosition, NetworkItemStackDescriptor,
};
use bedrock::protocol::v685::packets::ContainerClosePacket;
use bedrock::protocol::v712::enums::ContainerEnumName;
use bedrock::protocol::v729::types::FullContainerName;
use bedrock::protocol::v748::packets::InventoryContentPacket;

use crate::bedrock::protocol::Protocol;

/// 空槽位。`id == 0` 时编码器只写一个 `00`（后面的字段全部省略），与参考实现一致。
pub fn empty_item() -> NetworkItemStackDescriptor {
    NetworkItemStackDescriptor {
        id: 0,
        stack_size: None,
        aux_value: None,
        net_id_variant: None,
        block_runtime_id: None,
        user_data_buffer: None,
    }
}

/// `InventoryContent` 里那个"容器名"字段。Cloudburst 的默认值是
/// `FullContainerName(ContainerSlotType.ANVIL_INPUT /* = 0 */, null)`，
/// 也就是 `00 00`；Geyser 从没改过它，所以这里照抄。
fn default_container_name() -> FullContainerName<Protocol> {
    FullContainerName {
        container: ContainerEnumName::AnvilInputContainer,
        dynamic_id: None,
    }
}

fn content(
    container_id: ContainerID,
    slots: usize,
) -> InventoryContentPacket<Protocol> {
    InventoryContentPacket {
        inventory_id: container_id.0 as u32,
        slots: (0..slots).map(|_| empty_item()).collect(),
        container_name_data: default_container_name(),
        storage_item: empty_item(),
    }
}

/// 主背包 36 槽（9 快捷栏 + 27 背包）。
pub fn inventory_content() -> InventoryContentPacket<Protocol> {
    content(ContainerID::INVENTORY, 36)
}

/// 护甲 4 槽。
pub fn armor_content() -> InventoryContentPacket<Protocol> {
    content(ContainerID::ARMOR, 4)
}

/// 副手 1 槽。
pub fn offhand_content() -> InventoryContentPacket<Protocol> {
    content(ContainerID::OFFHAND, 1)
}

/// 打开背包界面。坐标取玩家脚下（Bedrock 坐标，y 允许为负，按 32 位二补数写无符号 varint）。
pub fn container_open(position: (f32, f32, f32)) -> ContainerOpenPacket<Protocol> {
    ContainerOpenPacket {
        container_id: ContainerID::INVENTORY,
        container_type: ContainerType::Inventory,
        position: NetworkBlockPosition {
            x: position.0.floor() as i32,
            y: position.1.floor() as i32 as u32,
            z: position.2.floor() as i32,
        },
        target_actor_id: ActorUniqueID(-1),
    }
}

/// 关背包的确认回执。客户端发来什么，就用同样的 id / `server_initiated_close` 回一份。
pub fn container_close(container_id: ContainerID, server_initiated: bool) -> ContainerClosePacket<Protocol> {
    ContainerClosePacket {
        container_id,
        container_type: ContainerType::Inventory,
        server_initiated_close: server_initiated,
    }
}

/// 一次"打开背包"要发的全部包，顺序与 Geyser 一致：先开容器，再灌内容。
pub fn open_inventory_packets(position: (f32, f32, f32)) -> Vec<Protocol> {
    vec![
        Protocol::ContainerOpenPacket(Box::new(container_open(position))),
        Protocol::InventoryContentPacket(Box::new(inventory_content())),
        Protocol::InventoryContentPacket(Box::new(armor_content())),
        Protocol::InventoryContentPacket(Box::new(offhand_content())),
    ]
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

    /// 期望字节来自 `refs/ProbeInventory.java`：
    /// `ContainerOpen(id=0, type=INVENTORY, uniqueEntityId=-1, pos=(128,64,128))`。
    #[test]
    fn container_open_matches_geyser_codec() {
        assert_eq!(
            hex(&container_open((128.5, 64.0, 128.5))),
            "00ff800240800201"
        );
    }

    /// 期望字节来自 `refs/ProbeInventory.java`：`InventoryContent(INVENTORY, 36 空)`，41 字节。
    #[test]
    fn inventory_content_matches_geyser_codec() {
        let h = hex(&inventory_content());
        assert_eq!(h.len(), 82, "包长不对：{h}");
        assert_eq!(
            h,
            "0024000000000000000000000000000000000000000000000000000000000000000000000000000000"
        );
    }

    /// 期望字节来自 `refs/ProbeInventory.java`：`InventoryContent(ARMOR, 4 空)` / `(OFFHAND, 1 空)`。
    #[test]
    fn armor_and_offhand_match_geyser_codec() {
        assert_eq!(hex(&armor_content()), "780400000000000000");
        assert_eq!(hex(&offhand_content()), "770100000000");
    }

    /// 关容器回执：`id=0`、`type=INVENTORY(-1)`、客户端发起 → `00 ff 00`。
    #[test]
    fn close_echo_roundtrips() {
        assert_eq!(hex(&container_close(ContainerID::INVENTORY, false)), "00ff00");
    }

    /// 打开背包必须是 1 个 ContainerOpen + 3 个 InventoryContent。
    #[test]
    fn open_sends_container_then_contents() {
        let packets = open_inventory_packets((0.5, -60.0, 0.5));
        assert_eq!(packets.len(), 4);
        assert!(matches!(packets[0], Protocol::ContainerOpenPacket(_)));
        assert!(matches!(packets[1], Protocol::InventoryContentPacket(_)));
    }
}
