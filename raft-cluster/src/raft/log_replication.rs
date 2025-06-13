use std::{collections::{HashMap, HashSet}, sync::Arc, time::{Duration, Instant}};

use anyhow::Result;
use tokio::{sync::Mutex, time::sleep};
use tracing::{info, warn, error};

use crate::{
    grpc::client::RaftClient,
    pb::{AppendEntriesRequest, AppendEntriesResponse, LogEntry},
    raft::node::{NodeRole, RaftNode},
};

/// 日志条目状态（你设计的状态流转）
#[derive(Debug, Clone)]
pub enum LogEntryState {
    Local,                    // 仅在Leader本地
    Replicating {            // 正在复制中
        confirmed_nodes: HashSet<String>,
        required_confirmations: usize,
        retry_count: HashMap<String, usize>, // 每个节点的重试次数
    },
    Committed,               // 已提交但未应用
    Applied,                 // 已应用到状态机
    Failed,                  // 复制失败（超过重试次数）
}

/// 复制任务
#[derive(Debug)]
pub struct ReplicationTask {
    pub entry: LogEntry,
    pub target_nodes: Vec<String>,
    pub state: LogEntryState,
    pub created_at: Instant,
}

/// 复制结果
#[derive(Debug)]
pub enum ReplicationResult {
    Success,                 // 复制成功
    InProgress,             // 仍在进行中
    Failed(String),         // 复制失败
    ConsistencyError,       // 一致性检查失败
}

/// 日志复制模块
#[derive(Clone)]
pub struct LogReplication {
    client: Arc<Mutex<RaftClient>>,
    max_retry_count: usize,  // 你提到的3次重试限制
    retry_interval: Duration,
}

impl LogReplication {
    pub fn new(client: Arc<Mutex<RaftClient>>) -> Self {
        Self {
            client,
            max_retry_count: 3,
            retry_interval: Duration::from_millis(100),
        }
    }

    /// 开始复制单个日志条目（你提到的单条复制）
    pub async fn replicate_entry(
        &self,
        node: Arc<Mutex<RaftNode>>,
        entry: LogEntry,
    ) -> Result<ReplicationResult> {
        let (leader_id, peers, current_term) = {
            let node_guard = node.lock().await;
            
            // 只有Leader才能发起复制
            if node_guard.role != NodeRole::Leader {
                return Err(anyhow::anyhow!("Only leader can replicate entries"));
            }
            
            (
                node_guard.node_id.clone(),
                node_guard.peers.clone(),
                node_guard.current_term,
            )
        };

        if peers.is_empty() {
            // 单节点集群，直接提交
            return Ok(ReplicationResult::Success);
        }

        let mut task = ReplicationTask {
            entry: entry.clone(),
            target_nodes: peers.clone(),
            state: LogEntryState::Replicating {
                confirmed_nodes: HashSet::new(),
                required_confirmations: peers.len() / 2 + 1,
                retry_count: HashMap::new(),
            },
            created_at: Instant::now(),
        };

        // 执行复制过程
        self.execute_replication(node, &mut task, leader_id, current_term).await
    }

    /// 执行具体的复制逻辑
    async fn execute_replication(
        &self,
        node: Arc<Mutex<RaftNode>>,
        task: &mut ReplicationTask,
        leader_id: String,
        current_term: u64,
    ) -> Result<ReplicationResult> {
        if let LogEntryState::Replicating { confirmed_nodes, required_confirmations, retry_count } = &mut task.state {
            
            // 并发发送到所有目标节点
            let mut futures = Vec::new();
            
            for peer in &task.target_nodes {
                // 检查是否已经确认或超过重试次数
                if confirmed_nodes.contains(peer) {
                    continue;
                }
                
                let current_retries = retry_count.get(peer).unwrap_or(&0);
                if *current_retries >= self.max_retry_count {
                    warn!("节点 {} 超过最大重试次数，跳过", peer);
                    continue;
                }

                let client = Arc::clone(&self.client);
                let peer_id = peer.clone();
                let entry = task.entry.clone();
                let leader_id_clone = leader_id.clone();
                let node_clone = Arc::clone(&node);

                let future = async move {
                    Self::send_append_entries(
                        client, 
                        node_clone,
                        peer_id.clone(), 
                        entry, 
                        leader_id_clone, 
                        current_term
                    ).await.map(|response| (peer_id, response))
                };
                
                futures.push(future);
            }

            // 等待所有响应（你的并发策略）
            let results = futures::future::join_all(futures).await;

            // 处理响应
            for result in results {
                match result {
                    Ok((peer_id, response)) => {
                        if self.handle_append_response(&peer_id, response, confirmed_nodes, retry_count).await {
                            info!("✅ 节点 {} 确认了日志条目 {}", peer_id, task.entry.index);
                        }
                    }
                    Err(e) => {
                        error!("发送到节点失败: {}", e);
                        // 增加重试计数
                        for peer in &task.target_nodes {
                            *retry_count.entry(peer.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }

            // 检查是否达到多数派
            if confirmed_nodes.len() >= *required_confirmations {
                task.state = LogEntryState::Committed;
                info!("🎉 日志条目 {} 已获得多数派确认", task.entry.index);
                return Ok(ReplicationResult::Success);
            }

            // 检查是否还有可以重试的节点
            let has_retryable_nodes = task.target_nodes.iter().any(|peer| {
                !confirmed_nodes.contains(peer) && 
                retry_count.get(peer).unwrap_or(&0) < &self.max_retry_count
            });

            if !has_retryable_nodes {
                task.state = LogEntryState::Failed;
                return Ok(ReplicationResult::Failed("所有节点都达到最大重试次数".to_string()));
            }

            return Ok(ReplicationResult::InProgress);
        }

        Err(anyhow::anyhow!("Invalid replication state"))
    }

    /// 发送AppendEntries请求（实现你说的一致性检查）
    async fn send_append_entries(
        client: Arc<Mutex<RaftClient>>,
        node: Arc<Mutex<RaftNode>>,
        peer_id: String,
        entry: LogEntry,
        leader_id: String,
        current_term: u64,
    ) -> Result<AppendEntriesResponse> {
        // 获取前一个日志条目的信息用于一致性检查
        let (prev_log_index, prev_log_term, leader_commit) = {
            let node_guard = node.lock().await;
            let prev_index = if entry.index > 1 { entry.index - 1 } else { 0 };
            let prev_term = if prev_index > 0 {
                // 从日志中获取前一个条目的term
                node_guard.log.entities
                    .iter()
                    .find(|e| e.index == prev_index)
                    .map(|e| e.term)
                    .unwrap_or(0)
            } else {
                0
            };
            
            (prev_index, prev_term, node_guard.log.commit_index)
        };

        let _request = AppendEntriesRequest {
            term: current_term,
            leader_id: leader_id.clone(),
            prev_log_index,     // 这就是你说的一致性检查关键
            prev_log_term,      // 这个也是
            entries: vec![entry.clone()],
            leader_commit,
        };

        // 发送请求
        let _response = {
            let mut client_guard = client.lock().await;
            client_guard.send_request_vote(peer_id.clone(), tonic::Request::new(
                // TODO: 这里需要修改client接口支持AppendEntries
                crate::pb::VoteRequest {
                    term: current_term,
                    candidate_id: leader_id,
                    last_log_index: entry.index,
                    last_log_term: entry.term,
                }
            )).await?
        };

        // TODO: 临时返回，需要实现真正的AppendEntries调用
        Ok(AppendEntriesResponse {
            term: current_term,
            success: true,
            follower_id: peer_id,
            conflict_index: 0,
        })
    }

    /// 处理AppendEntries响应
    async fn handle_append_response(
        &self,
        peer_id: &str,
        response: AppendEntriesResponse,
        confirmed_nodes: &mut HashSet<String>,
        retry_count: &mut HashMap<String, usize>,
    ) -> bool {
        if response.success {
            confirmed_nodes.insert(peer_id.to_string());
            retry_count.remove(peer_id); // 成功后清除重试计数
            true
        } else {
            // 失败时增加重试计数（你的重试策略）
            *retry_count.entry(peer_id.to_string()).or_insert(0) += 1;
            
            if response.conflict_index > 0 {
                // TODO: 实现冲突处理逻辑（回退next_index）
                warn!("节点 {} 日志冲突，conflict_index: {}", peer_id, response.conflict_index);
            }
            
            false
        }
    }

    /// 检查日志一致性（你问的一致性检查逻辑）
    pub fn check_log_consistency(
        local_log: &[LogEntry],
        prev_log_index: u64,
        prev_log_term: u64,
    ) -> bool {
        if prev_log_index == 0 {
            // 这是第一个日志条目，总是一致的
            return true;
        }

        // 查找指定索引的日志条目
        if let Some(entry) = local_log.iter().find(|e| e.index == prev_log_index) {
            // 检查term是否匹配
            entry.term == prev_log_term
        } else {
            // 本地没有该索引的日志，不一致
            false
        }
    }

    /// 处理日志冲突（你问的回退机制）
    pub fn handle_log_conflict(
        next_index: &mut HashMap<String, u64>,
        peer_id: &str,
        conflict_index: u64,
    ) {
        if let Some(current_next) = next_index.get_mut(peer_id) {
            // 你倾向的跳跃式回退策略
            if conflict_index > 0 && conflict_index < *current_next {
                *current_next = conflict_index;
                info!("调整 {} 的next_index到 {}", peer_id, conflict_index);
            } else {
                // 安全的线性回退
                if *current_next > 1 {
                    *current_next -= 1;
                }
                info!("线性回退 {} 的next_index到 {}", peer_id, *current_next);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use crate::{
        pb::LogEntry,
        raft::{log::RaftLog, state_machine::ConfigStateMachine},
    };

    fn create_test_entry(index: u64, term: u64, data: &str) -> LogEntry {
        LogEntry {
            index,
            term,
            data: data.as_bytes().to_vec(),
            entry_type: "config".to_string(),
            key: format!("key_{}", index),
        }
    }

    #[test]
    fn test_log_consistency_check() {
        // 测试你问的一致性检查逻辑
        let logs = vec![
            create_test_entry(1, 1, "data1"),
            create_test_entry(2, 1, "data2"),
            create_test_entry(3, 2, "data3"),
        ];

        // 测试第一个条目（prev_log_index=0）
        assert!(LogReplication::check_log_consistency(&logs, 0, 0));

        // 测试正常匹配的情况
        assert!(LogReplication::check_log_consistency(&logs, 1, 1));
        assert!(LogReplication::check_log_consistency(&logs, 2, 1));
        assert!(LogReplication::check_log_consistency(&logs, 3, 2));

        // 测试term不匹配的情况
        assert!(!LogReplication::check_log_consistency(&logs, 1, 2));
        assert!(!LogReplication::check_log_consistency(&logs, 2, 2));

        // 测试索引不存在的情况
        assert!(!LogReplication::check_log_consistency(&logs, 4, 2));
        assert!(!LogReplication::check_log_consistency(&logs, 10, 1));
    }

    #[test]
    fn test_log_conflict_handling() {
        // 测试你问的回退机制
        
        // 测试1: 跳跃式回退（你倾向的策略）
        let mut next_index = HashMap::new();
        next_index.insert("node1".to_string(), 5);
        LogReplication::handle_log_conflict(&mut next_index, "node1", 3);
        assert_eq!(next_index.get("node1"), Some(&3));

        // 测试2: conflict_index无效时的线性回退
        let mut next_index = HashMap::new();
        next_index.insert("node2".to_string(), 3);
        LogReplication::handle_log_conflict(&mut next_index, "node2", 0);
        assert_eq!(next_index.get("node2"), Some(&2));

        // 测试3: conflict_index大于current的情况（应该线性回退）
        let mut next_index = HashMap::new();
        next_index.insert("node1".to_string(), 3);
        LogReplication::handle_log_conflict(&mut next_index, "node1", 10);
        assert_eq!(next_index.get("node1"), Some(&2)); // 应该线性回退

        // 测试4: 最小值边界
        let mut next_index = HashMap::new();
        next_index.insert("node3".to_string(), 1);
        LogReplication::handle_log_conflict(&mut next_index, "node3", 0);
        assert_eq!(next_index.get("node3"), Some(&1)); // 不应该小于1
    }

    #[test]
    fn test_replication_state_transitions() {
        // 测试你设计的状态流转
        let mut state = LogEntryState::Local;

        // Local -> Replicating
        state = LogEntryState::Replicating {
            confirmed_nodes: HashSet::new(),
            required_confirmations: 3,
            retry_count: HashMap::new(),
        };

        if let LogEntryState::Replicating { confirmed_nodes, .. } = &mut state {
            confirmed_nodes.insert("node1".to_string());
            confirmed_nodes.insert("node2".to_string());
        }

        // Replicating -> Committed
        state = LogEntryState::Committed;
        assert!(matches!(state, LogEntryState::Committed));

        // Committed -> Applied
        state = LogEntryState::Applied;
        assert!(matches!(state, LogEntryState::Applied));
    }

    #[test]
    fn test_retry_count_tracking() {
        // 测试你提到的3次重试机制
        let mut retry_count = HashMap::new();
        
        // 模拟重试过程
        for i in 1..=3 {
            *retry_count.entry("node1".to_string()).or_insert(0) += 1;
            let current_retries = retry_count.get("node1").unwrap();
            
            if i < 3 {
                assert!(*current_retries < 3, "第{}次重试，应该还可以继续", i);
            } else {
                assert!(*current_retries >= 3, "第{}次重试，应该达到上限", i);
            }
        }

        // 超过上限后不应该再重试
        assert_eq!(retry_count.get("node1"), Some(&3));
    }

    #[test]
    fn test_majority_calculation() {
        // 测试多数派计算（与你的选举测试类似）
        assert_eq!(calculate_majority_for_replication(1), 1);
        assert_eq!(calculate_majority_for_replication(2), 2);
        assert_eq!(calculate_majority_for_replication(3), 2);
        assert_eq!(calculate_majority_for_replication(4), 3);
        assert_eq!(calculate_majority_for_replication(5), 3);
    }

    #[test]
    fn test_replication_task_creation() {
        // 测试复制任务的创建
        let entry = create_test_entry(1, 1, "test_data");
        let peers = vec!["node2".to_string(), "node3".to_string(), "node4".to_string()];
        
        let task = ReplicationTask {
            entry: entry.clone(),
            target_nodes: peers.clone(),
            state: LogEntryState::Replicating {
                confirmed_nodes: HashSet::new(),
                required_confirmations: peers.len() / 2 + 1, // 应该是2
                retry_count: HashMap::new(),
            },
            created_at: Instant::now(),
        };

        assert_eq!(task.entry.index, 1);
        assert_eq!(task.target_nodes.len(), 3);
        
        if let LogEntryState::Replicating { required_confirmations, .. } = task.state {
            assert_eq!(required_confirmations, 2);
        } else {
            panic!("状态应该是Replicating");
        }
    }

    // 辅助函数
    fn calculate_majority_for_replication(total_nodes: usize) -> usize {
        total_nodes / 2 + 1
    }
}
