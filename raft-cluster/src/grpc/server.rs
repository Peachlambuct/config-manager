use std::sync::Arc;

use tonic::{Request, Response, Status};
use tracing::{info, warn, error};

use crate::{
    pb::{
        config_service_server::ConfigService, raft_service_server::RaftService,
        AppendEntriesRequest, AppendEntriesResponse, GetClusterStateRequest,
        GetClusterStateResponse, ProposeConfigRequest, ProposeConfigResponse, ReadConfigRequest,
        ReadConfigResponse, VoteRequest, VoteResponse,
    },
    simple_raft::{RaftNode, ConfigRequest},
};

/// Raft服务实现 (使用OpenRaft)
pub struct RaftServiceImpl {
    raft_node: Arc<RaftNode>,
}

impl RaftServiceImpl {
    pub fn new(raft_node: Arc<RaftNode>) -> Self {
        Self { raft_node }
    }
}

#[tonic::async_trait]
impl RaftService for RaftServiceImpl {
    /// 处理投票请求
    async fn request_vote(
        &self,
        request: Request<VoteRequest>,
    ) -> Result<Response<VoteResponse>, Status> {
        let req = request.into_inner();

        info!(
            "📊 收到投票请求: candidate={}, term={}, last_log_index={}, last_log_term={}",
            req.candidate_id, req.term, req.last_log_index, req.last_log_term
        );

        // OpenRaft内部处理投票请求，这里返回基本响应
        // 在实际的OpenRaft网络层实现中，这会被正确路由
        warn!("🚧 投票请求暂时返回拒绝 - 需要实现OpenRaft网络层");
        
        let response = VoteResponse {
            term: req.term,
            vote_granted: false,
            voter_id: self.raft_node.node_id.to_string(),
        };

        Ok(Response::new(response))
    }

    /// 处理日志追加请求
    async fn append_entries(
        &self,
        request: Request<AppendEntriesRequest>,
    ) -> Result<Response<AppendEntriesResponse>, Status> {
        let req = request.into_inner();

        info!(
            "📝 收到日志追加请求: leader={}, term={}, prev_log_index={}, prev_log_term={}, entries={}",
            req.leader_id, req.term, req.prev_log_index, req.prev_log_term, req.entries.len()
        );

        // OpenRaft内部处理日志追加，这里返回基本响应
        warn!("🚧 日志追加请求暂时返回失败 - 需要实现OpenRaft网络层");
        
        let response = AppendEntriesResponse {
            term: req.term,
            success: false,
            follower_id: self.raft_node.node_id.to_string(),
            conflict_index: 0,
        };

        Ok(Response::new(response))
    }
}

/// 配置服务实现 (使用OpenRaft)
pub struct ConfigServiceImpl {
    raft_node: Arc<RaftNode>,
}

impl ConfigServiceImpl {
    pub fn new(raft_node: Arc<RaftNode>) -> Self {
        Self { raft_node }
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

        // 检查是否为Leader
        if !self.raft_node.is_leader().await {
            let metrics = self.raft_node.get_metrics().await;
            
            return Ok(Response::new(ProposeConfigResponse {
                success: false,
                message: "只有Leader可以处理写请求".to_string(),
                leader_id: metrics.current_leader.map(|id| id.to_string()).unwrap_or_default(),
            }));
        }

        // 构造配置请求（简化版本只支持set操作）
        if req.operation != "set" {
            return Ok(Response::new(ProposeConfigResponse {
                success: false,
                message: format!("当前只支持set操作，不支持: {}", req.operation),
                leader_id: self.raft_node.node_id.to_string(),
            }));
        }

        let config_request = ConfigRequest {
            key: req.key.clone(),
            value: String::from_utf8_lossy(&req.value).to_string(),
        };

        // 提交到Raft
        match self.raft_node.client_write(config_request).await {
            Ok(_response) => {
                info!("✅ 配置提议成功: {}", req.key);
                Ok(Response::new(ProposeConfigResponse {
                    success: true,
                    message: "配置提议成功".to_string(),
                    leader_id: self.raft_node.node_id.to_string(),
                }))
            }
            Err(e) => {
                error!("❌ 配置提议失败: {}", e);
                Ok(Response::new(ProposeConfigResponse {
                    success: false,
                    message: format!("配置提议失败: {}", e),
                    leader_id: self.raft_node.node_id.to_string(),
                }))
            }
        }
    }

    /// 读取配置
    async fn read_config(
        &self,
        request: Request<ReadConfigRequest>,
    ) -> Result<Response<ReadConfigResponse>, Status> {
        let req = request.into_inner();

        info!(
            "📖 收到配置读取请求: key={}, consistent_read={}",
            req.key, req.consistent_read
        );

        // 如果需要强一致性读取，检查是否是Leader
        if req.consistent_read && !self.raft_node.is_leader().await {
            let metrics = self.raft_node.get_metrics().await;
            
            return Ok(Response::new(ReadConfigResponse {
                success: false,
                value: vec![],
                message: format!(
                    "强一致性读取需要从Leader进行，当前Leader: {:?}", 
                    metrics.current_leader
                ),
                version: 0,
            }));
        }

        // 从状态机读取配置
        match self.raft_node.client_read(&req.key).await {
            Ok(Some(value)) => {
                info!("✅ 成功读取配置: key={}", req.key);
                Ok(Response::new(ReadConfigResponse {
                    success: true,
                    value: value.into_bytes(),
                    message: "配置读取成功".to_string(),
                    version: 1, // 简化版本号
                }))
            }
            Ok(None) => {
                info!("📖 配置不存在: {}", req.key);
                Ok(Response::new(ReadConfigResponse {
                    success: false,
                    value: vec![],
                    message: format!("配置项不存在: {}", req.key),
                    version: 0,
                }))
            }
            Err(e) => {
                error!("❌ 配置读取失败: key={}, error={}", req.key, e);
                Ok(Response::new(ReadConfigResponse {
                    success: false,
                    value: vec![],
                    message: format!("读取配置失败: {}", e),
                    version: 0,
                }))
            }
        }
    }

    /// 获取集群状态
    async fn get_cluster_state(
        &self,
        _request: Request<GetClusterStateRequest>,
    ) -> Result<Response<GetClusterStateResponse>, Status> {
        info!("🏥 收到集群状态查询请求");

        // 获取Raft指标
        let metrics = self.raft_node.get_metrics().await;
        
        // 构造节点信息
        let current_node = crate::pb::NodeInfo {
            node_id: self.raft_node.node_id.to_string(),
            address: "localhost:50051".to_string(), // TODO: 从配置获取实际地址
            role: if self.raft_node.is_leader().await {
                "leader".to_string()
            } else {
                "follower".to_string()
            },
            is_healthy: true,
            last_heartbeat: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        let response = GetClusterStateResponse {
            nodes: vec![current_node],
            leader_id: metrics.current_leader.map(|id| id.to_string()).unwrap_or_default(),
            current_term: metrics.current_term,
        };

        info!(
            "📊 返回集群状态: leader={}, term={}, nodes={}",
            response.leader_id,
            response.current_term,
            response.nodes.len()
        );

        Ok(Response::new(response))
    }
}
