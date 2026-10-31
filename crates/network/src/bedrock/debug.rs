//! 基岩版调试总开关（BUG-032）。
//!
//! **平时三个开关都是 `false`；排查时改 `true` 再重新编译**，就能看到逐包收发与明细日志。
//! 用"写死常量"而不是环境变量：服务端是手动起进程的，环境变量经常忘记带，
//! 而改这一行常量最显眼、不会漏。

// 注意：`Protocol`（= `V766`）实现的是 `Packets`（带 PacketHeader），**不是** `ProtoCodec`；
// 单独那些包结构体才实现 `ProtoCodec`。所以这里要导入 `Packets` + `PacketHeader`。
use bedrock::protocol::{PacketHeader, Packets};

use super::protocol::Protocol;

/// 逐包收发摘要：进/出站每个 Bedrock 包打一行（`[PKT] >> InventoryContent (52 B)`）。
pub const PACKETS: bool = false;

/// 出站包的 hex（截断到 [`HEX_LIMIT`] 字节）。只查某个包的字节布局时临时开。
pub const PACKET_HEX: bool = false;

/// 明细日志：`[--]` / `[S3]` / `[R5]` / `[A2]` / `[R7]` / `[R8]` / `[R10]` 这批逐事件日志
/// （登录取证、方块交互、物品事务、告示牌… 量很大，平时关）。
pub const VERBOSE: bool = false;

/// hex 打印上限（字节）。
const HEX_LIMIT: usize = 160;

/// 打进站一批包（`recv` 拿到 `batch` 后、转义/分发之前调用）。
pub fn dump_in(packets: &[Protocol]) {
    if !PACKETS {
        return;
    }
    for p in packets {
        println!("[PKT] << {}", super::session::pname(p));
    }
}

/// 打出站一批包（在 `conn.send(&packets)` 之前调用）。
///
/// 序列化失败只可能是内存问题，这里忽略错误（调试用，不能影响正常发包）。
pub fn dump_out(packets: &[Protocol]) {
    if !PACKETS {
        return;
    }
    for p in packets {
        // 带上 2 字节 `PacketHeader` 再序列化（`Protocol` 只实现 `Packets`），
        // 所以长度比真实负载多 2 字节；调试用无所谓，看相对大小即可。
        let mut buf = Vec::new();
        let _ = p.serialize(&PacketHeader::default(), &mut buf);
        println!("[PKT] >> {} ({} B)", super::session::pname(p), buf.len());
        if PACKET_HEX {
            let hex: String = buf
                .iter()
                .take(HEX_LIMIT)
                .map(|b| format!("{b:02x}"))
                .collect();
            println!("[PKT]    {hex}");
        }
    }
}

/// 明细日志：等价于 `if debug::VERBOSE { println!(...) }`。
///
/// 用法（`session.rs` 里已大量使用）：
/// ```ignore
/// crate::bedrock_verbose!("[R5] << ItemUse action={:?}", action);
/// ```
#[macro_export]
macro_rules! bedrock_verbose {
    ($($arg:tt)*) => {
        if $crate::bedrock::debug::VERBOSE {
            println!($($arg)*);
        }
    };
}
