//! FlowR A3T Controller
//!
//! Watches **Canvas** + **Runtime** custom resources (a3t.io/v1alpha1) and
//! reconciles them into k8s infra:
//! - Runtime → Deployment + Service + base IngressRoute + HPA  (Step 3, pending)
//! - Canvas  → placement/routing into a runtime, or a dedicated Deployment (Step 4, pending)
//!
//! This increment ships the CRD types + controllers + a status-only reconciler.
//! Triggers remain runtime artifacts (see A3T_CANVAS_SPEC.md §11).

pub mod api;
pub mod crd;
pub mod controller;
pub mod reconciler;
