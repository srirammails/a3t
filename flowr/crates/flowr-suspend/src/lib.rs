//! FlowR Suspend Manager
//!
//! Actor-based suspension system for Wait and Listen tasks.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     SuspendManager                          │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
//! │  │  FlowActor   │  │  FlowActor   │  │  FlowActor   │ ...  │
//! │  │  (waiting)   │  │  (listening) │  │  (waiting)   │      │
//! │  └──────────────┘  └──────────────┘  └──────────────┘      │
//! └─────────────────────────────────────────────────────────────┘
//!           │                   │                   │
//!           ▼                   ▼                   ▼
//!     Timer fires         Event matches        Timer fires
//!           │                   │                   │
//!           ▼                   ▼                   ▼
//!      Resume flow         Resume flow         Resume flow
//! ```
//!
//! # Suspension Types
//!
//! - **Wait**: Suspend for a duration, resume when timer expires
//! - **Listen**: Suspend until matching CloudEvent arrives
//!
//! # Dragonfly Schema
//!
//! ```text
//! flowr:suspend:{instance_id}     → JSON (SuspendedFlow)
//! flowr:suspend:timers            → SortedSet (instance_id, resume_at timestamp)
//! flowr:suspend:listeners:{type}  → Set of instance_ids listening for event type
//! ```

mod actor;
mod error;
mod event;
mod manager;
mod suspension;

pub use actor::FlowActor;
pub use error::{SuspendError, SuspendResult};
pub use event::{CloudEvent, EventFilter, EventMatcher};
pub use manager::SuspendManager;
pub use suspension::{ConsumptionStrategy, SuspendedFlow, SuspensionType};
