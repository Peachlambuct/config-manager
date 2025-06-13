use std::{collections::HashMap, time::Duration};

use tonic::{transport::Channel, Request, Response, Status};
use tracing::{error, info, warn};

use crate::pb::{
    raft_service_client::RaftServiceClient, AppendEntriesRequest, AppendEntriesResponse,
    VoteRequest, VoteResponse,
};

/// RaftClient错误类型
#[derive(Debug, thiserror::Error)]
pub enum RaftClientError {
    #[error("节点未找到: {0}")]
    NodeNotFound(String),
    #[error("连接失败: {0}")]
    ConnectionFailed(String),
    #[error("请求超时")]
    RequestTimeout,
    #[error("网络错误: {0}")]
    NetworkError(#[from] Status),
    #[error("重试次数超过限制")]
    RetryLimitExceeded,
    #[error("日志索引不匹配，需要回退")]
    LogIndexMismatch,
}

/// 客户端配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub connection_timeout: Duration,
    pub request_timeout: Duration,
    pub max_retry_count: usize,
    pub retry_interval: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connection_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(5),
            max_retry_count: 3,
            retry_interval: Duration::from_millis(200), // 固定间隔200ms
        }
    }
}

pub struct RaftClient {
    // 连接缓存 - 直接缓存RaftServiceClient
    clients: HashMap<String, RaftServiceClient<Channel>>,
    // 节点地址映射，用于重连
    node_addresses: HashMap<String, String>,
    // 客户端配置
    config: ClientConfig,
}

impl RaftClient {
    pub fn new() -> Self {
        Self::with_config(ClientConfig::default())
    }

    pub fn with_config(config: ClientConfig) -> Self {
        Self {
            clients: HashMap::new(),
            node_addresses: HashMap::new(),
            config,
        }
    }

    /// 连接到节点（建立连接缓存）
    pub async fn connect_to_node(&mut self, node_id: String, addr: String) -> Result<(), RaftClientError> {
        info!("🔗 连接到节点 {} ({})", node_id, addr);
        
        let endpoint = Channel::from_shared(addr.clone())
            .map_err(|e| RaftClientError::ConnectionFailed(e.to_string()))?;
        
        let channel = endpoint
            .connect_timeout(self.config.connection_timeout)
            .connect()
            .await
            .map_err(|e| RaftClientError::ConnectionFailed(e.to_string()))?;
        
        let client = RaftServiceClient::new(channel);
        self.clients.insert(node_id.clone(), client);
        self.node_addresses.insert(node_id.clone(), addr);
        
        info!("✅ 成功连接到节点 {}", node_id);
        Ok(())
    }

    /// 发送投票请求（带重试）
    pub async fn send_request_vote(
        &mut self,
        node_id: String,
        request: Request<VoteRequest>,
    ) -> Result<Response<VoteResponse>, RaftClientError> {
        let mut attempts = 0;
        
        loop {
            match self.try_send_request_vote(&node_id, request.get_ref().clone()).await {
                Ok(response) => return Ok(response),
                Err(RaftClientError::ConnectionFailed(_)) => {
                    // 连接失败直接返回，不重试
                    return Err(RaftClientError::ConnectionFailed(format!("无法连接到节点 {}", node_id)));
                }
                Err(RaftClientError::NetworkError(_)) | Err(RaftClientError::RequestTimeout) => {
                    // 网络错误和超时可以重试
                    attempts += 1;
                    if attempts >= self.config.max_retry_count {
                        return Err(RaftClientError::RetryLimitExceeded);
                    }
                    warn!("📡 投票请求失败，第 {} 次重试中...", attempts);
                    tokio::time::sleep(self.config.retry_interval).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// 实际发送投票请求
    async fn try_send_request_vote(
        &mut self,
        node_id: &str,
        request: VoteRequest,
    ) -> Result<Response<VoteResponse>, RaftClientError> {
        // 先提取配置避免借用问题
        let request_timeout = self.config.request_timeout;
        let client = self.get_or_reconnect_client(node_id).await?;
        
        let request = Request::new(request);
        let response = tokio::time::timeout(
            request_timeout,
            client.request_vote(request)
        ).await;
        
        match response {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(status)) => Err(RaftClientError::NetworkError(status)),
            Err(_) => Err(RaftClientError::RequestTimeout),
        }
    }

    /// 发送AppendEntries请求（核心方法，带完整重试逻辑）
    pub async fn send_append_entries(
        &mut self,
        node_id: &str,
        term: u64,
        leader_id: &str,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: Vec<crate::pb::LogEntry>,
        leader_commit: u64,
    ) -> Result<Response<AppendEntriesResponse>, RaftClientError> {
        let request = AppendEntriesRequest {
            term,
            leader_id: leader_id.to_string(),
            prev_log_index,
            prev_log_term,
            entries,
            leader_commit,
        };

        let mut attempts = 0;
        
        loop {
            match self.try_send_append_entries(node_id, &request).await {
                Ok(response) => {
                    let inner = response.get_ref();
                    
                    // 检查响应状态
                    if !inner.success && inner.conflict_index > 0 {
                        // 日志索引不匹配，需要回退
                        warn!("📋 节点 {} 日志索引不匹配，conflict_index: {}", node_id, inner.conflict_index);
                        return Err(RaftClientError::LogIndexMismatch);
                    }
                    
                    return Ok(response);
                }
                Err(RaftClientError::ConnectionFailed(_)) => {
                    // 连接失败直接返回
                    return Err(RaftClientError::ConnectionFailed(
                        format!("无法连接到节点 {}", node_id)
                    ));
                }
                Err(RaftClientError::NetworkError(_)) | Err(RaftClientError::RequestTimeout) => {
                    // 网络错误和超时进行重试
                    attempts += 1;
                    if attempts >= self.config.max_retry_count {
                        error!("🚫 向节点 {} 发送AppendEntries超过重试限制", node_id);
                        return Err(RaftClientError::RetryLimitExceeded);
                    }
                    warn!("🔄 AppendEntries失败，第 {} 次重试中...", attempts);
                    tokio::time::sleep(self.config.retry_interval).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// 实际发送AppendEntries请求
    async fn try_send_append_entries(
        &mut self,
        node_id: &str,
        request: &AppendEntriesRequest,
    ) -> Result<Response<AppendEntriesResponse>, RaftClientError> {
        // 先提取配置避免借用问题
        let request_timeout = self.config.request_timeout;
        let client = self.get_or_reconnect_client(node_id).await?;
        
        let request = Request::new(request.clone());
        let response = tokio::time::timeout(
            request_timeout,
            client.append_entries(request)
        ).await;
        
        match response {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(status)) => Err(RaftClientError::NetworkError(status)),
            Err(_) => Err(RaftClientError::RequestTimeout),
        }
    }

    /// 并发广播心跳（改进版）
    pub async fn broadcast_heartbeat(
        &mut self,
        request: AppendEntriesRequest,
    ) -> Vec<(String, Result<AppendEntriesResponse, RaftClientError>)> {
        let node_ids: Vec<String> = self.clients.keys().cloned().collect();
        let mut results = Vec::new();
        
        // 串行发送避免可变借用问题
        for node_id in node_ids {
            let result = self.send_append_entries(
                &node_id,
                request.term,
                &request.leader_id,
                request.prev_log_index,
                request.prev_log_term,
                request.entries.clone(),
                request.leader_commit,
            ).await.map(|resp| resp.into_inner());
            
            results.push((node_id, result));
        }
        
        results
    }

    /// 获取或重连客户端
    async fn get_or_reconnect_client(
        &mut self,
        node_id: &str,
    ) -> Result<&mut RaftServiceClient<Channel>, RaftClientError> {
        // 如果客户端不存在，尝试重连
        if !self.clients.contains_key(node_id) {
            if let Some(addr) = self.node_addresses.get(node_id).cloned() {
                warn!("🔄 重新连接到节点 {} ({})", node_id, addr);
                self.connect_to_node(node_id.to_string(), addr).await?;
            } else {
                return Err(RaftClientError::NodeNotFound(node_id.to_string()));
            }
        }
        
        Ok(self.clients.get_mut(node_id).unwrap())
    }

    /// 获取已连接的节点列表
    pub fn connected_nodes(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }

    /// 检查节点是否已连接
    pub fn is_connected(&self, node_id: &str) -> bool {
        self.clients.contains_key(node_id)
    }
}
