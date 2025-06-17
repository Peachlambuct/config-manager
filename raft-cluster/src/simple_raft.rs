use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// 节点ID类型
pub type NodeId = u64;

/// 应用数据类型 - 配置键值对
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigData {
    pub key: String,
    pub value: String,
}

/// 应用响应类型
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigResponse {
    pub success: bool,
    pub message: String,
}

/// 简化的Raft节点状态
#[derive(Debug, Clone, PartialEq)]
pub enum RaftState {
    Follower,
    Candidate,
    Leader,
}

/// 简化的Raft指标
#[derive(Debug, Clone)]
pub struct RaftMetrics {
    pub current_term: u64,
    pub current_leader: Option<NodeId>,
    pub state: RaftState,
    pub last_log_index: Option<u64>,
    pub last_applied: Option<u64>,
    pub membership_config: Vec<NodeId>,
}

/// 简化的Raft节点实现
/// 注意：这是一个简化的MVP版本，不是完整的Raft实现
/// 主要用于演示和快速原型开发
pub struct SimpleRaftNode {
    pub node_id: NodeId,
    
    // 内部状态
    state: Arc<RwLock<RaftState>>,
    current_term: Arc<RwLock<u64>>,
    current_leader: Arc<RwLock<Option<NodeId>>>,
    
    // 状态机 - 配置存储
    state_machine: Arc<RwLock<HashMap<String, String>>>,
    
    // 集群成员
    cluster_members: Arc<RwLock<Vec<NodeId>>>,
    
    // 启动时间（用于演示Leader选举）
    start_time: Instant,
}

impl SimpleRaftNode {
    /// 创建新的Raft节点
    pub async fn new(node_id: NodeId) -> Result<Self> {
        info!("🚀 创建简化Raft节点: {}", node_id);
        
        Ok(Self {
            node_id,
            state: Arc::new(RwLock::new(RaftState::Follower)),
            current_term: Arc::new(RwLock::new(0)),
            current_leader: Arc::new(RwLock::new(None)),
            state_machine: Arc::new(RwLock::new(HashMap::new())),
            cluster_members: Arc::new(RwLock::new(vec![])),
            start_time: Instant::now(),
        })
    }

    /// 初始化集群（简化版本）
    pub async fn initialize_cluster(&self, members: Vec<NodeId>) -> Result<()> {
        info!("🚀 初始化简化Raft集群，成员: {:?}", members);
        
        // 设置集群成员
        {
            let mut cluster_members = self.cluster_members.write().await;
            *cluster_members = members.clone();
        }
        
        // 简化的Leader选举：第一个节点或者ID最小的节点成为Leader
        let leader_id = *members.iter().min().unwrap_or(&self.node_id);
        
        if leader_id == self.node_id {
            info!("👑 节点 {} 成为Leader", self.node_id);
            *self.state.write().await = RaftState::Leader;
            *self.current_leader.write().await = Some(self.node_id);
            *self.current_term.write().await = 1;
        } else {
            info!("👥 节点 {} 成为Follower，Leader是 {}", self.node_id, leader_id);
            *self.state.write().await = RaftState::Follower;
            *self.current_leader.write().await = Some(leader_id);
            *self.current_term.write().await = 1;
        }
        
        Ok(())
    }

    /// 提交配置变更（简化版本）
    pub async fn client_write(&self, data: ConfigData) -> Result<ConfigResponse> {
        info!("📝 客户端写入请求: {:?}", data);
        
        // 检查是否为Leader
        if !self.is_leader().await {
            return Ok(ConfigResponse {
                success: false,
                message: "只有Leader可以处理写请求".to_string(),
            });
        }
        
        // 简化版本：直接写入状态机（跳过日志复制）
        {
            let mut state_machine = self.state_machine.write().await;
            state_machine.insert(data.key.clone(), data.value.clone());
        }
        
        info!("✅ 配置写入成功: {} = {}", data.key, data.value);
        
        Ok(ConfigResponse {
            success: true,
            message: "配置写入成功".to_string(),
        })
    }

    /// 读取配置（从状态机）
    pub async fn client_read(&self, key: &str) -> Result<Option<String>> {
        let state_machine = self.state_machine.read().await;
        let value = state_machine.get(key).cloned();
        Ok(value)
    }

    /// 获取集群状态
    pub async fn get_metrics(&self) -> RaftMetrics {
        let state = self.state.read().await.clone();
        let current_term = *self.current_term.read().await;
        let current_leader = *self.current_leader.read().await;
        let cluster_members = self.cluster_members.read().await.clone();
        let state_machine = self.state_machine.read().await;
        
        RaftMetrics {
            current_term,
            current_leader,
            state,
            last_log_index: Some(state_machine.len() as u64),
            last_applied: Some(state_machine.len() as u64),
            membership_config: cluster_members,
        }
    }

    /// 检查是否为Leader
    pub async fn is_leader(&self) -> bool {
        matches!(*self.state.read().await, RaftState::Leader)
    }

    /// 等待直到成为Leader或找到Leader（简化版本）
    pub async fn wait_for_leader(&self, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        
        loop {
            if start.elapsed() > timeout {
                return Err(anyhow::anyhow!("等待Leader超时"));
            }

            let current_leader = *self.current_leader.read().await;
            if current_leader.is_some() {
                info!("✅ 发现Leader: {:?}", current_leader);
                return Ok(());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// 添加学习者节点（简化版本 - 暂不实现）
    pub async fn add_learner(&self, id: NodeId) -> Result<()> {
        info!("📚 简化版本暂不支持动态添加学习者节点: {}", id);
        Ok(())
    }

    /// 变更集群成员（简化版本 - 暂不实现）
    pub async fn change_membership(&self, _members: Vec<NodeId>) -> Result<()> {
        info!("🗳️  简化版本暂不支持动态变更集群成员");
        Ok(())
    }

    /// 演示方法 - 批量设置配置
    pub async fn demo_set_configs(&self, configs: Vec<(String, String)>) -> Result<()> {
        if !self.is_leader().await {
            warn!("⚠️  当前节点不是Leader，无法写入配置");
            return Ok(());
        }

        for (key, value) in configs {
            let config_data = ConfigData { 
                key: key.clone(), 
                value: value.clone() 
            };
            
            match self.client_write(config_data).await {
                Ok(_) => info!("✅ 配置设置成功: {} = {}", key, value),
                Err(e) => warn!("❌ 配置设置失败: {} = {}, 错误: {}", key, value, e),
            }
            
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }

    /// 演示方法 - 读取配置
    pub async fn demo_read_configs(&self, keys: Vec<&str>) -> Result<()> {
        info!("📖 读取配置演示:");
        
        for key in keys {
            match self.client_read(key).await {
                Ok(Some(value)) => info!("  {} = {}", key, value),
                Ok(None) => info!("  {} = <未设置>", key),
                Err(e) => warn!("  {} = <读取失败: {}>", key, e),
            }
        }

        Ok(())
    }

    /// 获取所有配置（用于调试）
    pub async fn get_all_configs(&self) -> HashMap<String, String> {
        let state_machine = self.state_machine.read().await;
        state_machine.clone()
    }
}

// 为了兼容性，创建类型别名
pub type RaftNode = SimpleRaftNode;
pub type ConfigRequest = ConfigData;

impl std::fmt::Display for RaftState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RaftState::Follower => write!(f, "Follower"),
            RaftState::Candidate => write!(f, "Candidate"),
            RaftState::Leader => write!(f, "Leader"),
        }
    }
} 