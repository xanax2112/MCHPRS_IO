use crate::nbt_net::NbtCompound;
use crate::ProtoVersion;
use bedrock_macros::{packet, ProtoCodec};

#[packet(id = 56)]
#[derive(ProtoCodec, Clone, Debug)]
pub struct BlockActorDataPacket<V: ProtoVersion> {
    pub block_position: V::NetworkBlockPosition,
    // NOTE(local patch): 收发双向都改走 NbtCompound（无 facet）。
    pub actor_data_tags: NbtCompound,
}
