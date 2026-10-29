//! 基岩版（Bedrock）协议栈接入层。
//!
//! 与同为协议实现的 [`crate::packets`]（Java 协议）平级：`packets/` 放 Java 包定义，
//! 本模块放 Bedrock 侧的协议常量与包构造。
//!
//! 背景与验收记录见仓库根目录 `BACKLOG.md` 的 `BUG-032`，
//! 协议细节与踩坑证据见 `docs/BEDROCK-SPIKE-STATUS.md`。
//!
//! 设计约束（来自项目硬要求）：
//! - 不引入第三方运行时（无 JVM / 无 Geyser / 无 ViaProxy），只用 Rust 依赖。
//! - 任何二进制编码都必须先与开源参考实现（Geyser 内置的 Cloudburst v766 codec）
//!   逐字节对齐，见本模块的 `#[cfg(test)]` 回归测试。

pub mod protocol;

/// 本服务端锁定的 Bedrock 协议版本：766 = 1.21.51。
pub const PROTOCOL_VERSION: i32 = 766;

/// 客户端会在登录包里报告自己这一项，不匹配直接拒绝。
pub const GAME_VERSION: &str = "1.21.50";
