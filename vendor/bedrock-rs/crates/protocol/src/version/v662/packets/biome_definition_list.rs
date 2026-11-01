use crate::nbt_net::NbtCompound;
use bedrock_macros::{packet, ProtoCodec};

#[packet(id = 122, direction = "server_to_client")]
#[derive(ProtoCodec, Clone, Debug)]
pub struct BiomeDefinitionListPacket {
    // NOTE(local patch): upstream used `#[nbt] HashMap<String, nbtx::Value>`, which
    // serialises through facet's tagged pointers and aborts on Android (BUG-032 第 46 条).
    // `NbtCompound` is a hand-written ProtoCodec that never touches facet.
    pub biome_definition_data: NbtCompound,
}
