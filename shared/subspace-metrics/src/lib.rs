#![warn(missing_docs)]
//! This Rust module serves as a bridge between two different Prometheus metrics libraries
//! used within our frameworks — Substrate and Libp2p.
//! The module exposes a web server endpoint at "/metrics" that outputs metrics in Prometheus
//! format. It adapts metrics from either or both of the following libraries:
//! - Official Rust Prometheus client (registry aliased as Libp2pMetricsRegistry)
//! - TiKV's Prometheus client (registry aliased as SubstrateMetricsRegistry)

use actix_web::{get, web::Data, App, HttpResponse, HttpServer, http::StatusCode};
use parking_lot::Mutex;
use prometheus::{Encoder, Registry as SubstrateMetricsRegistry, TextEncoder};
use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry as Libp2pMetricsRegistry;
use std::error::Error;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info, warn};

type SharedRegistry = Arc<Mutex<RegistryAdapter>>;

/// An metrics registry adapter for Libp2p and Substrate frameworks.
/// It specifies which metrics registry or registries are in use.
pub enum RegistryAdapter {
    /// Uses only the Libp2p metrics registry.
    Libp2p(Libp2pMetricsRegistry),
    /// Uses only the Substrate metrics registry.
    Substrate(SubstrateMetricsRegistry),
    /// We use both Substrate and Libp2p metrics registries.
    Both(Libp2pMetricsRegistry, SubstrateMetricsRegistry),
}

#[get("/metrics")]
async fn metrics(registry: Data<SharedRegistry>) -> Result<HttpResponse, HttpResponse> {
    let encoded_metrics = match registry.lock().deref_mut() {
        RegistryAdapter::Libp2p(libp2p_registry) => encode_metrics_libp2p(libp2p_registry),
        RegistryAdapter::Substrate(substrate_registry) => encode_metrics_substrate(substrate_registry),
        RegistryAdapter::Both(libp2p_registry, substrate_registry) => {
            match (encode_metrics_libp2p(libp2p_registry), encode_metrics_substrate(substrate_registry)) {
                (Ok(libp2p_encoded), Ok(substrate_encoded)) => Ok(substrate_encoded + &libp2p_encoded),
                (Err(e), _) | (_, Err(e)) => Err(e),
            }
        }
    };

    encoded_metrics
        .map(|encoded| HttpResponse::build(StatusCode::OK).body(encoded))
        .map_err(|e| {
            error!("Failed to encode metrics: {}", e);
            HttpResponse::InternalServerError().finish()
        })
}

fn encode_metrics_libp2p(libp2p_registry: &Libp2pMetricsRegistry) -> Result<String, Box<dyn Error>> {
    let mut encoded = String::new();
    encode(&mut encoded, libp2p_registry)?;
    Ok(encoded)
}

fn encode_metrics_substrate(substrate_registry: &SubstrateMetricsRegistry) -> Result<String, Box<dyn Error>> {
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&substrate_registry.gather(), &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

/// Start prometheus metrics server on the provided address.
pub fn start_prometheus_metrics_server(
    endpoints: Vec<SocketAddr>,
    registry: RegistryAdapter,
) -> std::io::Result<impl Future<Output = std::io::Result<()>>> {
    let shared_registry = Arc::new(Mutex::new(registry));
    let data = Data::new(shared_registry);

    let app_factory = move || App::new().app_data(data.clone()).service(metrics);
    let result = HttpServer::new(app_factory.clone())
        .workers(2)
        .bind(endpoints.as_slice());

    let server = match result {
        Ok(server) => server,
        Err(error) => {
            if error.kind() != ErrorKind::AddrInUse {
                error!(?error, "Failed to start metrics server.");
                return Err(error);
            }
            recover_from_addr_in_use_error(endpoints, app_factory)
        }
    };

    info!(endpoints = ?server.addrs(), "Metrics server started.");

    Ok(server.run())
}

fn recover_from_addr_in_use_error(mut endpoints: Vec<SocketAddr>, app_factory: impl Fn() -> App + Clone) -> std::io::Result<HttpServer> {
    warn!("Address in use, attempting to start server with random port...");
    // Logic to recover from "address in use" error by setting port to 0
    endpoints.iter_mut().for_each(|endpoint| {
        endpoint.set_port(0);
    });

    HttpServer::new(app_factory)
        .workers(2)
        .bind
