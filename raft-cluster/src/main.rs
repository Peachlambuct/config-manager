use crate::pb::{
    config_service_server::ConfigServiceServer, raft_service_server::RaftServiceServer,
};
use anyhow::Result;
use std::net::SocketAddr;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber;

use crate::grpc::server::{ConfigServiceImpl, RaftServiceImpl};

mod grpc;
mod raft;
mod storage;

// 引入生成的gRPC代码
pub mod pb {
    tonic::include_proto!("raft");
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .init();

    info!("🚀 Starting Raft Cluster Node");

    let node_id = "node-1".to_string();
    let bind_address = "127.0.0.1:50051".to_string();

    let bind_address = bind_address.parse::<SocketAddr>()?;

    // tonic::transport::Server::builder()
    //     .add_service(RaftServiceServer::new(RaftServiceImpl::new(node_id.clone())))
    //     .add_service(ConfigServiceServer::new(ConfigServiceImpl::new(node_id)))
    //     .serve(bind_address)
    //     .await?;

    info!("👋 Shutting down...");

    Ok(())
}
