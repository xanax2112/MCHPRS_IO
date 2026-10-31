//! 起一个 Bedrock 端点，用于**协议层真机验证**（不需要启动整个 MCHPRS 服务端）。
//!
//! 运行：
//! ```sh
//! cargo run -p mchprs_network --example bedrock_server
//! ```
//! 然后在 Bedrock 客户端里连 `127.0.0.1:19132`（或本机局域网 IP）。
//!
//! 这里下发的是**石头测试世界**，刻意与 MCHPRS 真实世界分开（用户 2026-10-29 要求）：
//! 它只负责证明“登录 → StartGame → 区块 → 交互”这条链路在仓库代码里是通的。
//! MCHPRS 真实世界走 `session::spawn_mchprs_listener`，由 `crates/core` 在服务端启动时调用。

use mchprs_network::bedrock::session::{serve, WorldSource};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    serve("0.0.0.0:19132".to_string(), WorldSource::Demo).await
}
