use crate::ProtoVersion;
use bedrock_macros::ProtoCodec;

#[derive(ProtoCodec, Clone, Debug)]
#[enum_repr(i8)]
#[repr(i8)]
pub enum DataItemType<V: ProtoVersion> {
    Byte(i8) = 0,
    Short(#[endianness(le)] i16) = 1,
    Int(#[endianness(var)] i32) = 2,
    Float(#[endianness(le)] f32) = 3,
    String(String) = 4,
    NBT(#[nbt] nbtx::Value) = 5,
    Pos(V::BlockPos) = 6,
    // PATCH-005：LONG 型实体元数据在线上是“无符号”varint，不是 zigzag。
    // 证据：用云暴（Cloudburst）v766 codec 序列化同一组标志，得到的是无符号 LEB128
    // （例：BREATHING 单独置位 → 80 80 80 80 80 02 = 1<<36），而上游按 i64(zigzag)
    // 编码会把数值整体乘 2，客户端收到的标志位全部错位。
    Int64(#[endianness(var)] u64) = 7,
    Vec3(#[endianness(le)] (f32, f32, f32)) = 8,
}
