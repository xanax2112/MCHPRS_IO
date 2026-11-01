use crate::nbt_net::NbtCompound;
use bedrock_macros::{packet, ProtoCodec};

#[packet(id = 119, direction = "server_to_client")]
#[derive(ProtoCodec, Clone, Debug)]
pub struct AvailableActorIdentifiersPacket {
    // NOTE(local patch): 同 BiomeDefinitionListPacket，改走 NbtCompound（无 facet）。
    pub actor_info_list: NbtCompound,
}
