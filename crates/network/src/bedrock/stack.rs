//! 插头：把整套基岩版协议栈装进插座。
//!
//! 只有启用 `bedrock` feature 时这个文件才会被编译（见 `mod.rs` 的 `#[cfg]`）。
//! 根二进制的 `main()` 调 [`install`] 把 [`Stack`] 注册进 [`crate::bedrock_api`]；
//! 不启用 feature 时没人调用它，`bedrock_api::stack()` 就是 `None`——
//! 服务端退化成一台纯 Java 服务器，UDP 19132 不监听，12,500 行协议代码根本不参与编译。

use std::sync::Arc;

use crate::bedrock_api::{
    BedrockEvent, BedrockSign, BedrockStack, ChunkEncoder,
};
use crate::bedrock::{chunk, session};

/// 基岩版协议栈（插头本体）。
pub struct Stack;

impl BedrockStack for Stack {
    fn spawn(
        &self,
        addr: String,
        events: std::sync::mpsc::Sender<BedrockEvent>,
        view_radius: i32,
    ) {
        session::spawn_mchprs_listener(addr, events, view_radius);
    }

    fn chunk_encoder(&self) -> Arc<dyn ChunkEncoder> {
        Arc::new(Stack)
    }

    fn block_state(&self, java_state: u32) -> i32 {
        crate::bedrock::bedrock_block_state(java_state)
    }

    fn java_item_id(&self, bedrock_item_id: i32) -> Option<u32> {
        crate::bedrock::mchprs_item_id(bedrock_item_id)
    }

    fn bedrock_item_id(&self, java_item_id: u32) -> Option<i32> {
        crate::bedrock::bedrock_item_id(java_item_id)
    }
}

impl ChunkEncoder for Stack {
    fn y_offset(&self) -> i32 {
        chunk::Y_OFFSET
    }

    fn initial_radius_chunks(&self) -> i32 {
        chunk::INITIAL_RADIUS_CHUNKS
    }

    fn encode(
        &self,
        get_block: &mut dyn FnMut(i32, i32, i32) -> u32,
        chunk_x: i32,
        chunk_z: i32,
        biome_id: i32,
        signs: &[BedrockSign],
    ) -> Vec<u8> {
        chunk::build_chunk_from_world_with_signs(
            |x, y, z| get_block(x, y, z),
            chunk_x,
            chunk_z,
            biome_id,
            signs,
        )
    }

    fn pending_chunks(
        &self,
        center: (i32, i32),
        radius: i32,
        have: &std::collections::HashSet<(i32, i32)>,
    ) -> Vec<(i32, i32)> {
        chunk::pending_chunks(center, radius, have)
    }

    fn covered_radius(
        &self,
        center: (i32, i32),
        radius: i32,
        have: &std::collections::HashSet<(i32, i32)>,
    ) -> i32 {
        chunk::covered_radius(center, radius, have)
    }

    fn stream_budget_chunks(&self) -> usize {
        chunk::STREAM_BUDGET_CHUNKS
    }

    fn stream_budget_ms(&self) -> u64 {
        chunk::STREAM_BUDGET_MS
    }

    fn section_count(&self) -> i32 {
        chunk::BEDROCK_SECTION_COUNT
    }
}

/// 把插头装进插座。由根二进制的 `main()` 在启用 `bedrock` feature 时调用一次。
pub fn install() {
    crate::bedrock_api::install(Arc::new(Stack));
}
