pub mod grpc;
pub mod raft;
pub mod storage;
 
// 引入生成的gRPC代码
pub mod pb {
    tonic::include_proto!("raft");
} 