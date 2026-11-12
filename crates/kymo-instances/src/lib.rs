//! Challenge instance lifecycle. `spec` and `policy` are pure rules; `runtime` is the
//! platform interface with an in memory fake; `docker` (feature) talks to a Docker daemon.

mod error;
mod policy;
mod runtime;
mod spec;

#[cfg(feature = "docker")]
pub mod docker;
pub mod hardening;

pub use error::{RuntimeError, SpecError};
pub use hardening::{ALLOWED_CAPS, Hardening};
pub use policy::{
    Action, Denied, InstanceRecord, InstanceState, Policy, Usage, authorize, due_for_reaping,
    effective_state, expires_at,
};
pub use runtime::{
    Endpoint, ExtraPort, FakeRuntime, FlagDelivery, InstanceRuntime, Managed, Placement, RunFlag,
    RuntimeStatus, StartExtras, StartOutcome, StartRequest, Started, VerifierSpec,
};
pub use spec::{InstanceSpec, ResourceCaps};
