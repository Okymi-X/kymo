//! Challenge instance lifecycle. `spec` and `policy` are pure rules; `runtime` is the
//! platform interface with an in memory fake; `docker` (feature) talks to a Docker daemon.

mod error;
mod policy;
mod runtime;
mod spec;

#[cfg(feature = "docker")]
pub mod docker;

pub use error::{RuntimeError, SpecError};
pub use policy::{
    Action, Denied, InstanceRecord, InstanceState, Policy, Usage, authorize, due_for_reaping,
    effective_state, expires_at,
};
pub use runtime::{
    FakeRuntime, InstanceRuntime, Managed, Placement, RuntimeStatus, StartRequest, Started,
};
pub use spec::{InstanceSpec, ResourceCaps};
