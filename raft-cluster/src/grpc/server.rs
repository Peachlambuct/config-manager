use std::sync::Arc;
use tokio::sync::Mutex;

use tonic::{Request, Response, Status};
use tracing::{info, warn, error};

use crate::{
    grpc::client::RaftClient,
    pb::{
        config_service_server::ConfigService, raft_service_server::RaftService,
        AppendEntriesRequest, AppendEntriesResponse, GetClusterStateRequest,
        GetClusterStateResponse, ProposeConfigRequest, ProposeConfigResponse,
        ReadConfigRequest, ReadConfigResponse, VoteRequest, VoteResponse,
    },
    raft::{engine::{RaftEngine, ClusterInfo}, node::NodeRole},
};

/// Raft服务实现
pub struct RaftServiceImpl {
    engine: Arc<Mutex<RaftEngine>>,
    client: Arc<RaftClient>,
}

impl RaftServiceImpl {
    pub fn new(engine: Arc<Mutex<RaftEngine>>, client: Arc<RaftClient>) -> Self {
        Self { engine, client }
    }
}

#[tonic::async_trait]
impl RaftService for RaftServiceImpl {
    /// 处理投票请求 - 实现真正的Raft投票逻辑
    async fn request_vote(
        &self,
        request: Request<VoteRequest>,
    ) -> Result<Response<VoteResponse>, Status> {
        let req = request.into_inner();

        info!(
            "📊 收到投票请求: candidate={}, term={}, last_log_index={}, last_log_term={}",
            req.candidate_id, req.term, req.last_log_index, req.last_log_term
        );

        // 使用RaftEngine的深度集成方法处理投票请求
        let response = self.engine.lock().await.handle_vote_request(&req).await;

        info!(
            "🗳️  投票结果: candidate={}, granted={}, term={}",
            req.candidate_id, response.vote_granted, response.term
        );

        Ok(Response::new(response))
    }

    /// 处理日志追加请求 - 实现真正的Raft日志追加逻辑
    async fn append_entries(
        &self,
        request: Request<AppendEntriesRequest>,
    ) -> Result<Response<AppendEntriesResponse>, Status> {
        let req = request.into_inner();

        info!(
            "📝 收到日志追加请求: leader={}, term={}, prev_log_index={}, prev_log_term={}, entries={}",
            req.leader_id, req.term, req.prev_log_index, req.prev_log_term, req.entries.len()
        );

        // 使用RaftEngine的深度集成方法处理AppendEntries请求
        let response = self.engine.lock().await.handle_append_entries(&req).await;

        let status_msg = if response.success { "成功" } else { "失败" };
        info!(
            "📝 AppendEntries处理{}: follower={}, term={}, conflict_index={}",
            status_msg, response.follower_id, response.term, response.conflict_index
        );

        Ok(Response::new(response))
    }
}

/// 配置服务实现
pub struct ConfigServiceImpl {
    engine: Arc<Mutex<RaftEngine>>,
    client: Arc<RaftClient>,
}

impl ConfigServiceImpl {
    pub fn new(engine: Arc<Mutex<RaftEngine>>, client: Arc<RaftClient>) -> Self {
        Self { engine, client }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigServiceImpl {
    /// 提议配置变更
    async fn propose_config(
        &self,
        request: Request<ProposeConfigRequest>,
    ) -> Result<Response<ProposeConfigResponse>, Status> {
        let req = request.into_inner();

        info!(
            "⚙️ 收到配置提议: key={}, operation={}",
            req.key, req.operation
        );

        let success = self
            .engine
            .lock()
            .await
            .propose_config(req.key, req.value)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let leader_id = self.engine.lock().await.get_leader_id().await.unwrap_or_default();

        let response = ProposeConfigResponse {
            success,
            message: if success { "配置提议成功".to_string() } else { "配置提议失败".to_string() },
            leader_id,
        };

        Ok(Response::new(response))
    }

    /// 读取配置
    async fn read_config(
        &self,
        request: Request<ReadConfigRequest>,
    ) -> Result<Response<ReadConfigResponse>, Status> {
        let req = request.into_inner();

        info!("📖 收到配置读取请求: key={}, consistent_read={}", req.key, req.consistent_read);

        // 如果需要强一致性读取，检查是否是Leader
        if req.consistent_read {
            let role = self.engine.lock().await.get_role().await;
            if role != NodeRole::Leader {
                let leader_id = self.engine.lock().await.get_leader_id().await.unwrap_or_default();
                return Ok(Response::new(ReadConfigResponse {
                    success: false,
                    value: vec![],
                    message: format!("强一致性读取需要从Leader进行，当前Leader: {}", leader_id),
                    version: 0,
                }));
            }
        }

        // 从状态机读取配置
        let result = self.read_config_from_state_machine(&req.key).await;
        
        let response = match result {
            Ok((value, version)) => {
                info!("✅ 成功读取配置: key={}, version={}", req.key, version);
                ReadConfigResponse {
                    success: true,
                    value,
                    message: "配置读取成功".to_string(),
                    version,
                }
            }
            Err(msg) => {
                warn!("❌ 配置读取失败: key={}, error={}", req.key, msg);
                ReadConfigResponse {
                    success: false,
                    value: vec![],
                    message: msg,
                    version: 0,
                }
            }
        };

        Ok(Response::new(response))
    }

    /// 获取集群状态
    async fn get_cluster_state(
        &self,
        _request: Request<GetClusterStateRequest>,
    ) -> Result<Response<GetClusterStateResponse>, Status> {
        info!("🏥 收到集群状态查询请求");

        // 获取集群状态信息
        let cluster_state = self.collect_cluster_state().await;

        let response = GetClusterStateResponse {
            nodes: cluster_state.nodes,
            leader_id: cluster_state.leader_id,
            current_term: cluster_state.current_term,
        };

        info!(
            "📊 返回集群状态: leader={}, term={}, nodes={}",
            response.leader_id, response.current_term, response.nodes.len()
        );

        Ok(Response::new(response))
    }
}

impl ConfigServiceImpl {
    /// 从状态机读取配置
    async fn read_config_from_state_machine(&self, key: &str) -> Result<(Vec<u8>, u64), String> {
        // 使用RaftEngine的深度集成方法
        self.engine.lock().await.read_config_from_state_machine(key).await
    }

    /// 收集集群状态信息
    async fn collect_cluster_state(&self) -> ClusterStateInfo {
        // 使用RaftEngine的深度集成方法获取集群信息
        let cluster_info = self.engine.lock().await.get_cluster_info().await;
        
        // 构造当前节点信息
        let current_node = crate::pb::NodeInfo {
            node_id: cluster_info.node_id.clone(),
            address: "localhost:50051".to_string(), // TODO: 从配置读取实际地址
            role: role_to_string(cluster_info.role),
            is_healthy: true,
            last_heartbeat: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        // TODO: 这里应该收集所有节点的信息
        // 可以通过RaftClient查询其他节点状态
        let mut nodes = vec![current_node];
        
        // 为其他peers创建基础节点信息
        for peer in &cluster_info.peers {
            if peer != &cluster_info.node_id {
                nodes.push(crate::pb::NodeInfo {
                    node_id: peer.clone(),
                    address: format!("{}:50051", peer), // TODO: 从配置读取实际地址
                    role: "unknown".to_string(), // TODO: 查询实际状态
                    is_healthy: false, // TODO: 健康检查
                    last_heartbeat: 0,
                });
            }
        }

        ClusterStateInfo {
            nodes,
            leader_id: cluster_info.leader_id.unwrap_or_default(),
            current_term: cluster_info.current_term,
        }
    }
}

/// 集群状态信息
struct ClusterStateInfo {
    nodes: Vec<crate::pb::NodeInfo>,
    leader_id: String,
    current_term: u64,
}

/// 将NodeRole转换为字符串
fn role_to_string(role: NodeRole) -> String {
    match role {
        NodeRole::Leader => "leader".to_string(),
        NodeRole::Follower => "follower".to_string(),
        NodeRole::Candidate => "candidate".to_string(),
    }
}
