//! Controllers — watch Canvas + Runtime CRs and drive reconciliation.
//!
//! Runs as the `flowr-controller` binary (separate from flowr-server, EE only).

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use kube::runtime::controller::{Action, Controller};
use kube::runtime::watcher::Config;
use kube::{Api, Client};
use tracing::{error, info, warn};

use crate::crd::{Canvas, Runtime};
use crate::reconciler::{reconcile_canvas, reconcile_runtime, ReconcileContext, ReconcileError};

type WErr = kube::runtime::watcher::Error;

/// Start the Canvas + Runtime controllers (runs forever).
pub async fn run(client: Client) -> anyhow::Result<()> {
    let ctx = Arc::new(ReconcileContext { client: client.clone() });
    info!("Starting A3T Canvas + Runtime controllers");

    let canvases: Api<Canvas> = Api::all(client.clone());
    let runtimes: Api<Runtime> = Api::all(client.clone());

    let canvas_ctrl = Controller::new(canvases, Config::default())
        .run(
            |obj, ctx| async move {
                match reconcile_canvas(obj, ctx).await {
                    Ok(()) => Ok(Action::requeue(Duration::from_secs(30))),
                    Err(e) => {
                        error!(error = %e, "canvas reconcile failed");
                        Ok(Action::requeue(Duration::from_secs(10)))
                    }
                }
            },
            |_obj: Arc<Canvas>, error: &kube::runtime::controller::Error<ReconcileError, WErr>, _ctx: Arc<ReconcileContext>| {
                error!("canvas controller error: {:?}", error);
                Action::requeue(Duration::from_secs(5))
            },
            ctx.clone(),
        )
        .for_each(|_| async {});

    let runtime_ctrl = Controller::new(runtimes, Config::default())
        .run(
            |obj, ctx| async move {
                match reconcile_runtime(obj, ctx).await {
                    Ok(()) => Ok(Action::requeue(Duration::from_secs(30))),
                    Err(e) => {
                        error!(error = %e, "runtime reconcile failed");
                        Ok(Action::requeue(Duration::from_secs(10)))
                    }
                }
            },
            |_obj: Arc<Runtime>, error: &kube::runtime::controller::Error<ReconcileError, WErr>, _ctx: Arc<ReconcileContext>| {
                error!("runtime controller error: {:?}", error);
                Action::requeue(Duration::from_secs(5))
            },
            ctx.clone(),
        )
        .for_each(|_| async {});

    // The in-cluster HTTP API flowr (EE) pushes canvases through.
    let api_addr = std::env::var("A3T_CONTROLLER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let api_client = client.clone();
    let api_task = async move {
        match api_addr.parse() {
            Ok(addr) => {
                if let Err(e) = crate::api::serve(api_client, addr).await {
                    error!(error = %e, "controller HTTP API exited");
                }
            }
            Err(e) => warn!(addr = %api_addr, error = %e, "invalid A3T_CONTROLLER_ADDR — HTTP API disabled"),
        }
    };

    // Drive both controllers + the HTTP API concurrently, forever.
    tokio::join!(canvas_ctrl, runtime_ctrl, api_task);
    Ok(())
}
