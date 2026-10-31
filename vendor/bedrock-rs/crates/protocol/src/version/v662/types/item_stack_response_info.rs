// PATCH-008（BUG-032 B1）：`ItemStackResponseInfo` 的解码原来是 run-on 写法。
//
// 线上字节顺序是：`result(1 字节) | clientRequestId(varint) | containers(...)`，
// 而 `containers` **只有 `result == Success(0)` 时才存在**（Cloudburst 的
// `ItemStackNetResult.Success` 自己携带 `List<ItemStackResponseContainer>`）。
//
// 上游原来的 `deserialize` 用 `stream.read_to_end(...)` 把包**余下的所有字节**都当成
// "第一个 response 的容器" —— 一旦 `responses.len() > 1`，第二个 response 就永远读不到，
// 而且会把整包吃掉。这里改成"按每个 response 自己的容器数量读"，不再 run-on。
//
// 参考字节见 `refs/ProbeItemStackResponse.java`（Cloudburst v766）与
// `crates/network/src/bedrock/protocol.rs` 的 `item_stack_response_decodes_cloudburst_bytes`。
use crate::ProtoVersion;
use bedrock_protocol_core::error::ProtoCodecError;
use bedrock_protocol_core::{ProtoCodec, ProtoCodecVAR};
use byteorder::{ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read, Write, copy};

#[derive(Clone, Debug)]
pub struct ItemStackResponseInfo<V: ProtoVersion> {
    pub result: V::ItemStackNetResult,
    pub client_request_id: i32,
}

/// `ItemStackNetResult` 里"成功"那个变体的线上值（Cloudburst `Success = 0`）。
const RESULT_SUCCESS: i8 = 0;

impl<V: ProtoVersion> ProtoCodec for ItemStackResponseInfo<V> {
    fn serialize<W: Write>(&self, stream: &mut W) -> Result<(), ProtoCodecError> {
        // 结果码先写出去；`Success` 变体自身带着容器列表，所以先在临时缓冲里编一遍，
        // 把第 1 个字节（结果码）拿出去，其余字节（容器列表）留到 requestId 之后再写。
        let mut result_stream: Vec<u8> = Vec::new();
        self.result.serialize(&mut result_stream)?;
        let mut result_cursor = Cursor::new(result_stream.as_slice());

        stream.write_i8(result_cursor.read_i8()?)?;
        <i32 as ProtoCodecVAR>::serialize(&self.client_request_id, stream)?;
        copy(&mut result_cursor, stream)?;

        Ok(())
    }

    fn deserialize<R: Read>(stream: &mut R) -> Result<Self, ProtoCodecError> {
        let result_byte = stream.read_i8()?;
        let client_request_id = <i32 as ProtoCodecVAR>::deserialize(stream)?;

        // 只把"这一个 response 自己的"容器读出来，绝不吃后面的 response。
        // 结果码 0（Success）后面才跟 `Vec<ItemStackResponseContainerInfo>`。
        //
        // 不能直接 `V::ItemStackNetResult::Success(vec)` —— 这是**关联类型**，
        // 变体名在泛型上下文里不可见。所以把 `结果码字节 + 容器列表编码` 拼回一个
        // 小缓冲，再交给枚举自己的解码器（多一次容器重编码，量极小）。
        let mut rebuf: Vec<u8> = Vec::new();
        rebuf.write_i8(result_byte)?;
        if result_byte == RESULT_SUCCESS {
            let containers: Vec<V::ItemStackResponseContainerInfo> = ProtoCodec::deserialize(stream)?;
            ProtoCodec::serialize(&containers, &mut rebuf)?;
        }

        let mut rebuf_cursor = Cursor::new(rebuf.as_slice());
        let result = <V::ItemStackNetResult as ProtoCodec>::deserialize(&mut rebuf_cursor)?;

        Ok(Self {
            result,
            client_request_id,
        })
    }

    fn size_hint(&self) -> usize {
        // `result` 的 size_hint 已经含容器列表（Success 变体带载荷）。
        self.result.size_hint() + <i32 as ProtoCodecVAR>::size_hint(&self.client_request_id)
    }
}
