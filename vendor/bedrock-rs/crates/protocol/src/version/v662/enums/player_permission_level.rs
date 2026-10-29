use bedrock_macros::ProtoCodec;

// NOTE(local patch PATCH-004): 协议里 PlayerPermissionLevel 是单字节。
// 上游写的是 `#[enum_repr(i32)] #[enum_endianness(var)]`，而 var 对 i32 走 zigzag，
// 导致 Member(1) 被序列化成 0x02（即 Operator），客户端会把自己当成 OP。
#[derive(ProtoCodec, Clone, Debug)]
#[enum_repr(i8)]
#[repr(i8)]
pub enum PlayerPermissionLevel {
    Visitor = 0,
    Member = 1,
    Operator = 2,
    Custom = 3,
}
