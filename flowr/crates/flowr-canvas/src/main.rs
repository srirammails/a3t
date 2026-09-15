//! FlowR A3T Controller — watches Canvas + Runtime CRs and reconciles them.
//!
//! ```bash
//! flowr-controller            # run controllers (in-cluster or KUBECONFIG)
//! flowr-controller --crd      # print the Canvas + Runtime CRD YAML (for install)
//! ```

use kube::CustomResourceExt;
use tracing::info;
use tracing_subscriber::EnvFilter;

use flowr_canvas::crd::{Canvas, Runtime};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // `--crd`: emit the CRD manifests and exit (no cluster needed).
    if args.iter().any(|a| a == "--crd") {
        print!("{}", serde_yaml::to_string(&Canvas::crd())?);
        println!("---");
        print!("{}", serde_yaml::to_string(&Runtime::crd())?);
        return Ok(());
    }

    // Install rustls crypto provider before any TLS usage.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    info!("A3T Controller starting");

    let client = kube::Client::try_default().await?;
    let version = client.apiserver_version().await?;
    info!(version = %format!("{}.{}", version.major, version.minor), "Connected to Kubernetes");

    flowr_canvas::controller::run(client).await?;
    Ok(())
}
