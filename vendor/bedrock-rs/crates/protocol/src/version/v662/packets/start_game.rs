use std::collections::HashMap;
use crate::ProtoVersion;
use bedrock_macros::{packet, ProtoCodec};
use uuid::Uuid;

#[packet(id = 11, direction = "server_to_client")]
#[derive(ProtoCodec, Clone, Debug)]
pub struct StartGamePacket<V: ProtoVersion> {
    pub target_actor_id: V::ActorUniqueID,
    pub target_runtime_id: V::ActorRuntimeID,
    pub actor_game_type: V::GameType,
    #[endianness(le)]
    pub position: (f32, f32, f32),
    #[endianness(le)]
    pub rotation: (f32, f32),
    pub settings: V::LevelSettings,
    pub level_id: String,
    pub level_name: String,
    pub template_content_identity: String,
    pub is_trial: bool,
    pub movement_settings: V::SyncedPlayerMovementSettings,
    #[endianness(le)]
    pub current_level_time: u64,
    #[endianness(var)]
    pub enchantment_seed: i32,
    pub block_properties: Vec<BlockProperty>,
    pub item_list: Vec<V::ItemData>,
    pub multiplayer_correlation_id: String,
    pub enable_item_stack_net_manager: bool,
    pub server_version: String,
    pub player_property_data: PlayerPropertyData,
    #[endianness(le)]
    pub server_block_type_registry_checksum: u64,
    pub world_template_id: Uuid,
    pub server_enabled_client_side_generation: bool,
    pub block_network_ids_are_hashes: bool,
    pub network_permissions: V::NetworkPermissions,
}

/// NOTE(local patch): upstream used `#[nbt] HashMap<String, nbtx::Value>` here.
/// nbtx/facet 0.50 serialises via facet's tagged pointers (bit 63 of a pointer),
/// which aborts on Android because bionic/Scudo already uses the pointer's top byte
/// as a tag ("Pointer tag for 0x34... was truncated"). This newtype writes the same
/// NBT shape (an NBT compound) without going through facet, and is limited to the
/// empty map that the Bedrock server actually sends.
#[derive(Clone, Debug, Default)]
pub struct PlayerPropertyData(pub HashMap<String, nbtx::Value>);

impl bedrock_protocol_core::ProtoCodec for PlayerPropertyData {
    fn serialize<W: std::io::Write>(
        &self,
        stream: &mut W,
    ) -> Result<(), bedrock_protocol_core::error::ProtoCodecError> {
        if !self.0.is_empty() {
            return Err(bedrock_protocol_core::error::ProtoCodecError::FormatMismatch(
                "PlayerPropertyData: non-empty properties unsupported by local patch",
            ));
        }
        // Bedrock 的 nbt 字段：无长度前缀，字符串长度用 varint（littleVarint）。
        // 空 compound = TAG_Compound(0x0A) + 空名字(0x00) + TAG_End(0x00) = 3 字节。
        stream.write_all(&[0x0A, 0x00, 0x00])?;
        Ok(())
    }

    fn deserialize<R: std::io::Read>(
        stream: &mut R,
    ) -> Result<Self, bedrock_protocol_core::error::ProtoCodecError> {
        use std::io::Read;
        // 仅支持空 compound：0x0A + 名字长度 varint(0) + TAG_End
        let mut head = [0u8; 2];
        stream.read_exact(&mut head)?;
        if head[0] != 0x0A || head[1] != 0x00 {
            return Err(bedrock_protocol_core::error::ProtoCodecError::FormatMismatch(
                "PlayerPropertyData: only empty compound is supported by local patch",
            ));
        }
        let mut end = [0u8; 1];
        stream.read_exact(&mut end)?;
        Ok(Self(HashMap::new()))
    }

    fn size_hint(&self) -> usize {
        3
    }
}

#[derive(ProtoCodec, Clone, Debug)]
pub struct BlockProperty {
    pub block_name: String,
    #[nbt]
    pub block_definition: HashMap<String, nbtx::Value>,
}
