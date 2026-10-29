//! 起一个 Bedrock 端点，用于**真机验证**协议层是否工作（不需要启动整个 MCHPRS 服务端）。
//!
//! 运行：
//! ```sh
//! cargo run -p mchprs_network --example bedrock_server
//! ```
//! 然后在 Bedrock 客户端里连 `127.0.0.1:19132`（或本机局域网 IP）。
//!
//! 目前下发的是 spike 阶段的最小世界（一层石头 + 空气），目的只是证明
//! 登录 → StartGame → 区块 → 交互这条链路在**仓库代码**里是通的；
//! 真实世界数据接入见 `docs/bedrock-integration.md` §6。

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    mchprs_network::bedrock::session::serve("0.0.0.0:19132".to_string()).await
}
