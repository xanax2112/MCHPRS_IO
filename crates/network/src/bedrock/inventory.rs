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

/// 带内容的一批槽位（BUG-032 第 39 条：进 plot 时把存档里的背包发给客户端）。
///
/// `items` 是 **MCHPRS 物品 id + 数量**，`None` = 空槽；映射成 Bedrock 物品 id 走
/// `entity::held_item`（查不到的物品一律发空气，绝不拿 Java id 顶替）。
pub fn content_with(
    container: ContainerID,
    items: &[Option<(u32, u8)>],
) -> InventoryContentPacket<Protocol> {
    InventoryContentPacket {
        inventory_id: container.0 as u32,
        slots: items
            .iter()
            .map(|item| crate::bedrock::entity::held_item(*item))
            .collect(),
        container_name_data: default_container_name(),
        storage_item: empty_item(),
    }
}

/// BUG-032 第 44 条：一次 `InventoryTransaction(NORMAL)` 是不是"丢物品"，要不要改镜像背包。
///
/// **形状出处**（Geyser `BedrockInventoryTransactionTranslator`）：
/// `NORMAL` 事务 + 两条 action —— `actions[0]` 的来源是 `WORLD_INTERACTION`
/// （物品被"投入世界"），`actions[1]` 是背包里那一格（`slot` 是快捷栏 0..=8）。
/// Geyser 用 `worldInteraction.getFlag() == Flag.DROP_ITEM` 判丢，丢整叠还是丢一个看
/// `worldAction.getToItem().getCount() > 1`；这里改用**数量必须变小**来判，判据更硬：
/// 捡东西（`PICKUP_ITEM`）会让那一格变大，天然被排除。
///
/// 我们没有掉落物实体（MCHPRS 世界根本不放实体），所以唯一要做的事是：
/// 把**服务端镜像的背包**改成丢完的样子，否则切栏 / 重进时 `InventoryContent`
/// 会把丢掉的东西"复活"。
///
/// 返回 `(快捷栏槽 0..=8, 该格剩余物品)`；`None` = 不是丢物品 / 不需要同步。
/// 剩余物品是 `None` 表示那一格已经空了。物品查不到 MCHPRS id 时返回 `None`
/// （宁可不同步，也不能把剩下一叠东西误删 —— 绝不瞎编 id）。
pub fn drop_update(
    actions: &[bedrock::protocol::v662::types::InventoryAction<Protocol>],
) -> Option<(u32, Option<(u32, u8)>)> {
    if actions.len() != 2 {
        return None;
    }
    let world = &actions[0];
    let container = &actions[1];
    // 只有「世界交互」来源才可能是丢/捡；容器来源的是普通搬物（属 A7，不做）。
    let is_world = matches!(
        &world.source.source_type,
        bedrock::protocol::v662::enums::InventorySourceType::WorldInteraction(_)
    );
    if !is_world || container.slot > 8 {
        return None;
    }
    let from = stack_total(&container.from_item_descriptor);
    let to = stack_total(&container.to_item_descriptor);
    if to >= from || from == 0 {
        return None;
    }
    let item = match &container.to_item_descriptor.id {
        0 => None,
        id => {
            let count = container
                .to_item_descriptor
                .stack_size
                .unwrap_or(1)
                .clamp(1, 255) as u8;
            Some((crate::bedrock::mchprs_item_id(*id)?, count))
        }
    };
    Some((container.slot, item))
}

/// 描述符里那一叠到底有多少（`id == 0` 就是空槽，编码器连 `stack_size` 都不写）。
fn stack_total(item: &NetworkItemStackDescriptor) -> u32 {
    if item.id == 0 {
        0
    } else {
        item.stack_size.unwrap_or(1) as u32
    }
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
///
/// ⚠️ **不要拿它给真实玩家用**（BUG-032 第 45 条）：这里的三份 `InventoryContent`
/// 是**全空**的，而我们是客户端权威背包，客户端会把空内容当成权威 → 一开背包东西全没了。
/// 现在开界面只发 `container_open`，见 `session.rs`。
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

    /// BUG-032 第 39 条：回传背包时非空槽要真的带上物品（Java 物品 id → Bedrock 物品 id），
    /// 空槽仍然是 `id = 0`。
    #[test]
    fn content_with_maps_items_and_keeps_empty_slots() {
        let mut items: Vec<Option<(u32, u8)>> = vec![None; 36];
        items[0] = Some((1, 12)); // 石头：Java 1 → Bedrock 1
        items[9] = Some((331, 3)); // glowstone（Java 331）→ Bedrock 89
        let packet = content_with(ContainerID::INVENTORY, &items);
        assert_eq!(packet.slots.len(), 36);
        assert_eq!(packet.slots[0].id, 1);
        assert_eq!(packet.slots[0].stack_size, Some(12));
        assert_eq!(packet.slots[9].id, 89);
        assert_eq!(packet.slots[9].stack_size, Some(3));
        assert_eq!(packet.slots[1].id, 0, "空槽必须是空气");
    }

    /// 打开背包必须是 1 个 ContainerOpen + 3 个 InventoryContent。
    #[test]
    fn open_sends_container_then_contents() {
        let packets = open_inventory_packets((0.5, -60.0, 0.5));
        assert_eq!(packets.len(), 4);
        assert!(matches!(packets[0], Protocol::ContainerOpenPacket(_)));
        assert!(matches!(packets[1], Protocol::InventoryContentPacket(_)));
    }

    // ---- BUG-032 第 44 条：丢物品 ----

    fn descriptor(id: i32, count: u16) -> NetworkItemStackDescriptor {
        if id == 0 {
            return empty_item();
        }
        NetworkItemStackDescriptor {
            id,
            stack_size: Some(count),
            aux_value: Some(0),
            net_id_variant: Some(None),
            block_runtime_id: Some(0),
            user_data_buffer: Some(Vec::new()),
        }
    }

    /// 造一条 action。`world = true` → 来源是 `WorldInteraction(0)`（丢物品），
    /// 否则来源是背包容器。
    fn action(
        world: bool,
        slot: u32,
        from: (i32, u16),
        to: (i32, u16),
    ) -> bedrock::protocol::v662::types::InventoryAction<Protocol> {
        use bedrock::protocol::v662::enums::InventorySourceType;
        use bedrock::protocol::v662::types::{InventoryAction, InventorySource};
        InventoryAction {
            source: InventorySource {
                source_type: if world {
                    InventorySourceType::WorldInteraction(0)
                } else {
                    InventorySourceType::ContainerInventory(0)
                },
            },
            slot,
            from_item_descriptor: descriptor(from.0, from.1),
            to_item_descriptor: descriptor(to.0, to.1),
        }
    }

    /// 从一叠 5 个里丢 1 个 → 镜像背包剩 4 个；丢整叠 → 那一格清空。
    #[test]
    fn drop_updates_mirror_stack() {
        // Bedrock 石头 = 1，MCHPRS 石头 = 1
        let single = [action(true, 2, (1, 5), (1, 4)), action(false, 2, (1, 5), (1, 4))];
        assert_eq!(drop_update(&single), Some((2, Some((1, 4)))));

        let all = [action(true, 2, (1, 5), (1, 5)), action(false, 2, (1, 5), (0, 0))];
        assert_eq!(drop_update(&all), Some((2, None)));
    }

    /// 捡东西（那一格变大）、容器搬运（来源不是世界）、超过两条 action 都不算丢物品。
    #[test]
    fn drop_ignores_pickup_and_container_moves() {
        let pickup = [action(true, 0, (1, 2), (1, 3)), action(false, 0, (1, 2), (1, 3))];
        assert_eq!(drop_update(&pickup), None, "变大 = 捡，不是丢");

        let chest = [action(false, 0, (1, 2), (1, 1)), action(false, 0, (1, 2), (1, 1))];
        assert_eq!(drop_update(&chest), None, "容器搬运不归这里管（A7）");

        let three = [
            action(true, 0, (1, 2), (1, 1)),
            action(false, 0, (1, 2), (1, 1)),
            action(false, 0, (1, 2), (1, 1)),
        ];
        assert_eq!(drop_update(&three), None);
        assert_eq!(drop_update(&[]), None);
    }

    /// 移动端快捷栏只有 0..=8；越界槽位不猜。
    #[test]
    fn drop_ignores_out_of_range_slot() {
        let bad = [action(true, 40, (1, 5), (1, 4)), action(false, 40, (1, 5), (1, 4))];
        assert_eq!(drop_update(&bad), None);
    }
}
