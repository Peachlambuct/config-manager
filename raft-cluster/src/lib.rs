pub mod grpc;
pub mod storage;
pub mod config;
pub mod cluster;
pub mod simple_raft;  // 使用简化的raft实现
 
// 引入生成的gRPC代码
pub mod pb {
    tonic::include_proto!("raft");
} 