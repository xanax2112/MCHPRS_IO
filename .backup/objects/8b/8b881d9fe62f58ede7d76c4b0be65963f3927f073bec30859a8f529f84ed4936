pub mod direct;

use std::sync::Arc;

use super::compile_graph::CompileGraph;
use super::task_monitor::TaskMonitor;
use super::CompilerOptions;
use enum_dispatch::enum_dispatch;
use mchprs_blocks::BlockPos;
use mchprs_world::{TickEntry, World};

/// An IO element that can be driven or observed through the automation port.
/// The element codes are part of the wire protocol, do not reorder them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoElement {
    Button,
    Lever,
    PressurePlate,
    Lamp,
    Trapdoor,
    NoteBlock,
}

impl IoElement {
    /// Stable wire code used by the automation protocol.
    pub fn code(self) -> u8 {
        match self {
            IoElement::Button => 0,
            IoElement::Lever => 1,
            IoElement::PressurePlate => 2,
            IoElement::Lamp => 3,
            IoElement::Trapdoor => 4,
            IoElement::NoteBlock => 5,
        }
    }
}

/// A state change of an IO element detected by the backend.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    pub pos: BlockPos,
    pub element: IoElement,
    /// Powered or lit
    pub powered: bool,
    pub output_power: u8,
}

#[enum_dispatch]
pub trait JITBackend {
    fn compile(
        &mut self,
        graph: CompileGraph,
        ticks: Vec<TickEntry>,
        options: &CompilerOptions,
        monitor: Arc<TaskMonitor>,
    );
    fn tick(&mut self);

    fn tickn(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.tick();
        }
    }

    fn on_use_block(&mut self, pos: BlockPos);
    fn set_pressure_plate(&mut self, pos: BlockPos, powered: bool);
    fn flush<W: World>(&mut self, world: &mut W, io_only: bool);
    fn reset<W: World>(&mut self, world: &mut W, io_only: bool);
    fn has_pending_ticks(&self) -> bool;
    /// Inspect block for debugging
    fn inspect(&mut self, pos: BlockPos);

    /// Returns `(element, powered, output_power)` for the IO node at `pos`.
    /// Returns `None` when there is no IO node at `pos`.
    fn query_io(&self, pos: BlockPos) -> Option<(IoElement, bool, u8)>;
    /// Triggers a button pulse or a lever toggle at `pos`.
    /// Returns `false` when there is no button/lever node at `pos`.
    fn try_use_block(&mut self, pos: BlockPos) -> bool;
    /// Sets the absolute state of a lever or pressure plate at `pos`.
    /// Returns `false` when there is no lever/pressure plate node at `pos`.
    fn try_set_powered(&mut self, pos: BlockPos, powered: bool) -> bool;
    /// Drains all pending IO state change events.
    fn take_io_events(&mut self) -> Vec<IoEvent>;
}

use direct::DirectBackend;

#[enum_dispatch(JITBackend)]
pub enum BackendDispatcher {
    DirectBackend,
}
