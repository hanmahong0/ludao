pub mod sidecar;
pub mod supervisor;
pub mod zeroclaw_rpc;

#[cfg(test)]
mod integration_tests;

pub use supervisor::{
    assistant_cancel, assistant_run_draft, assistant_set_enabled, assistant_status, AssistantSupervisor,
};