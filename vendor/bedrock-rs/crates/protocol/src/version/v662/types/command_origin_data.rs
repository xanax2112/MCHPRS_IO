use crate::ProtoVersion;
use bedrock_protocol_core::ProtoCodec;
use bedrock_protocol_core::error::ProtoCodecError;
use std::io::{Read, Write};
use uuid::Uuid;

/// 命令来源（玩家输入的 `/xxx`、命令方块、控制台……）。
///
/// **PATCH-006（本地补丁，2026-10-31）**：上游这里用的是"run-on"写法 ——
/// 先把 `command_type` 写进临时 buffer，读出它的 varint 编号写进主流，
/// 紧接着写 uuid / requestId，最后用 `read_to_end` 把**剩下的所有字节**塞回临时 buffer。
///
/// `read_to_end` 在解码时会**吃掉整个包剩余的所有字节**，于是 `CommandRequestPacket`
/// 后面的 `internal` / `version` 必然读到 EOF：
///
/// ```text
/// 真机：客户端输入 /tp 就掉线
/// [!!] 收包失败: failed to de/serialize packet CommandRequestPacket (id: 77):
///                 IOError occurred: failed to fill whole buffer
/// ```
///
/// 权威字节来自 Cloudburst v766（`refs/ProbeCommandRequest.java`）：
/// `0e「/tp 150 20 150」 00 uuid(16) 05「req-1」 00 68`，共 40 字节 ——
/// 编号、uuid、requestId 是**紧挨着顺序排列**的，没有任何 run-on 结构。
/// 所以这里改成按顺序读写（编号 varint → uuid → requestId），与参考实现逐字节一致。
#[derive(Clone, Debug)]
pub struct CommandOriginData<V: ProtoVersion> {
    pub command_type: V::CommandOriginType,
    pub command_uuid: Uuid,
    pub request_id: String,
}

impl<V: ProtoVersion> ProtoCodec for CommandOriginData<V> {
    fn serialize<W: Write>(&self, stream: &mut W) -> Result<(), ProtoCodecError> {
        self.command_type.serialize(stream)?;
        self.command_uuid.serialize(stream)?;
        self.request_id.serialize(stream)?;

        Ok(())
    }

    fn deserialize<R: Read>(stream: &mut R) -> Result<Self, ProtoCodecError> {
        let command_type = V::CommandOriginType::deserialize(stream)?;
        let command_uuid = Uuid::deserialize(stream)?;
        let request_id = String::deserialize(stream)?;

        Ok(Self {
            command_type,
            command_uuid,
            request_id,
        })
    }

    fn size_hint(&self) -> usize {
        self.command_type.size_hint() + self.command_uuid.size_hint() + self.request_id.size_hint()
    }
}
