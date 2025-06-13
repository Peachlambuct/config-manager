use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, sleep};
use tracing::{error, info, warn};
use std::collections::HashMap;

use crate::{
    grpc::client::{RaftClient, RaftClientError},
    pb::{LogEntry, VoteRequest, VoteResponse, AppendEntriesRequest, AppendEntriesResponse},
    raft::{
        leader_election::{ElectionResult, LeaderElection},
        log::RaftLog,
        log_replication::{LogEntryState, LogReplication, ReplicationResult},
        node::{NodeRole, RaftNode},
        state_machine::ConfigStateMachine,
    },
};

pub struct RaftEngine {
    node: Arc<Mutex<RaftNode>>,
    leader_election: LeaderElection,
    log_replication: LogReplication,
    client: Arc<Mutex<RaftClient>>,
    running: Arc<RwLock<bool>>,
}

impl RaftEngine {
    pub fn new(node: RaftNode, client: RaftClient) -> Self {
        let node_arc = Arc::new(Mutex::new(node));
        let client_arc = Arc::new(Mutex::new(client));

        Self {
            leader_election: LeaderElection::new(client_arc.clone()),
            log_replication: LogReplication::new(client_arc.clone()),
            node: node_arc,
            client: client_arc,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动Raft引擎
    pub async fn start(&self) -> Result<()> {
        println!("启动Raft引擎...");

        {
            let mut running = self.running.write().await;
            *running = true;
        }

        // 启动主循环
        let engine_clone = self.clone();
        tokio::spawn(async move {
            engine_clone.run_main_loop().await;
        });

        Ok(())
    }

    /// 停止Raft引擎
    pub async fn stop(&self) -> Result<()> {
        println!("停止Raft引擎...");
        let mut running = self.running.write().await;
        *running = false;
        Ok(())
    }

    /// Raft引擎主循环
    async fn run_main_loop(self) {
        let mut election_timer = interval(Duration::from_millis(100));
        let mut heartbeat_timer = interval(Duration::from_millis(50));

        loop {
            // 检查是否需要停止
            {
                let running = self.running.read().await;
                if !*running {
                    break;
                }
            }

            let role = {
                let node = self.node.lock().await;
                node.role
            };

            match role {
                NodeRole::Follower => {
                    tokio::select! {
                        _ = election_timer.tick() => {
                            if self.should_start_election().await {
                                self.start_election().await;
                            }
                        }
                    }
                }
                NodeRole::Candidate => {
                    // 候选人状态通常在选举过程中处理
                    sleep(Duration::from_millis(10)).await;
                }
                NodeRole::Leader => {
                    tokio::select! {
                        _ = heartbeat_timer.tick() => {
                            self.send_heartbeats().await;
                        }
                    }
                }
            }

            // 短暂休眠避免CPU占用过高
            sleep(Duration::from_millis(10)).await;
        }
    }

    /// 检查是否应该开始选举
    async fn should_start_election(&self) -> bool {
        let node = self.node.lock().await;
        match node.role {
            NodeRole::Follower => {
                // 检查选举超时
                node.election_timeout < Instant::now()
            }
            _ => false,
        }
    }

    /// 开始选举
    async fn start_election(&self) {
        println!("开始选举...");

        match self.leader_election.start_election(self.node.clone()).await {
            Ok(ElectionResult::Won) => {
                println!("选举获胜，成为Leader");
                self.become_leader().await;
            }
            Ok(ElectionResult::Lost) => {
                println!("选举失败，回到Follower状态");
                self.become_follower(None).await;
            }
            Ok(ElectionResult::TermUpdated(new_term)) => {
                println!("发现更高任期: {}，更新任期", new_term);
                self.become_follower(None).await;
            }
            Err(e) => {
                println!("选举过程出错: {}", e);
                self.become_follower(None).await;
            }
        }
    }

    /// 成为Leader
    async fn become_leader(&self) {
        let last_log_index = {
            let node = self.node.lock().await;
            node.log.last_log_index()
        };

        let mut node = self.node.lock().await;
        node.role = NodeRole::Leader;
        node.leader_id = Some(node.node_id.clone());

        // 初始化next_index和match_index
        let peers = node.peers.clone();
        for peer in &peers {
            if peer != &node.node_id {
                node.next_index
                    .insert(peer.clone(), last_log_index + 1);
                node.match_index.insert(peer.clone(), 0);
            }
        }

        println!("成为Leader，当前任期: {}", node.current_term);
    }

    /// 成为Follower
    async fn become_follower(&self, leader_id: Option<String>) {
        let mut node = self.node.lock().await;
        node.role = NodeRole::Follower;
        node.leader_id = leader_id;
        node.voted_for = None;

        // 重置选举超时
        use rand::Rng;
        let timeout_ms = rand::thread_rng().gen_range(150..=300);
        node.election_timeout = Instant::now() + Duration::from_millis(timeout_ms);

        println!("成为Follower");
    }

    /// 发送心跳
    async fn send_heartbeats(&self) {
        let (peers, term, leader_id, prev_log_index, prev_log_term, leader_commit) = {
            let node = self.node.lock().await;
            if node.role != NodeRole::Leader {
                return;
            }

            (
                node.peers.clone(),
                node.current_term,
                node.node_id.clone(),
                node.log.last_log_index(),
                node.log.last_log_term(),
                node.log.commit_index,
            )
        };

        // 使用改进的心跳发送机制
        let mut successful_heartbeats = 0;
        let mut failed_nodes = Vec::new();

        for peer in &peers {
            if peer != &leader_id {
                let result = {
                    let mut client = self.client.lock().await;
                    client.send_append_entries(
                        peer,
                        term,
                        &leader_id,
                        prev_log_index,
                        prev_log_term,
                        vec![], // 空entries表示心跳
                        leader_commit,
                    ).await
                };

                match result {
                    Ok(_) => {
                        successful_heartbeats += 1;
                        info!("💗 成功发送心跳到节点 {}", peer);
                    }
                    Err(RaftClientError::ConnectionFailed(_)) => {
                        error!("🔌 无法连接到节点 {}，可能已下线", peer);
                        failed_nodes.push(peer.clone());
                    }
                    Err(RaftClientError::RetryLimitExceeded) => {
                        warn!("⏰ 向节点 {} 发送心跳重试超限", peer);
                        failed_nodes.push(peer.clone());
                    }
                    Err(RaftClientError::LogIndexMismatch) => {
                        warn!("📋 节点 {} 日志索引不匹配，需要同步", peer);
                        // 这里可以触发日志同步逻辑
                        self.handle_log_mismatch(peer).await;
                    }
                    Err(e) => {
                        error!("❌ 向节点 {} 发送心跳失败: {}", peer, e);
                        failed_nodes.push(peer.clone());
                    }
                }
            }
        }

        // 检查是否失去了多数派连接
        let total_peers = peers.len();
        let required_majority = total_peers / 2 + 1;
        
        if successful_heartbeats + 1 < required_majority { // +1 是自己
            warn!("⚠️  失去多数派连接，考虑退位为Follower");
            // 在实际实现中，这里可能需要更复杂的逻辑
            // 比如设置一个计数器，连续几次失去多数派后才退位
        }

        if !failed_nodes.is_empty() {
            warn!("🔄 心跳发送失败的节点: {:?}", failed_nodes);
        }
    }

    /// 处理日志索引不匹配的情况
    async fn handle_log_mismatch(&self, peer_id: &str) {
        warn!("🔧 处理节点 {} 的日志不匹配", peer_id);
        
        // 获取该节点的next_index并回退
        {
            let mut node = self.node.lock().await;
            if let Some(next_index) = node.next_index.get_mut(peer_id) {
                if *next_index > 1 {
                    *next_index -= 1;
                    info!("📉 节点 {} 的next_index回退到 {}", peer_id, *next_index);
                }
            }
        }

        // 注意：这里不立即触发同步，而是在下次心跳时自然处理
        // 这样可以避免递归调用的问题
    }

    /// 向特定节点同步日志
    async fn sync_logs_to_peer(&self, peer_id: &str) {
        info!("🔄 开始向节点 {} 同步日志", peer_id);
        
        // 获取需要同步的日志条目
        let (entries, term, leader_id, prev_log_index, prev_log_term, leader_commit) = {
            let node = self.node.lock().await;
            if node.role != NodeRole::Leader {
                return;
            }

            let next_index = node.next_index.get(peer_id).copied().unwrap_or(1);
            let entries = node.log.get_entries_from(next_index);
            
            (
                entries,
                node.current_term,
                node.node_id.clone(),
                next_index.saturating_sub(1),
                node.log.get_term_at(next_index.saturating_sub(1)).unwrap_or(0),
                node.log.commit_index,
            )
        };

        let entries_len = entries.len();

        // 发送日志条目
        let result = {
            let mut client = self.client.lock().await;
            client.send_append_entries(
                peer_id,
                term,
                &leader_id,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit,
            ).await
        };

        match result {
            Ok(response) => {
                let resp = response.into_inner();
                if resp.success {
                    info!("✅ 成功向节点 {} 同步日志", peer_id);
                    // 更新match_index和next_index
                    let mut node = self.node.lock().await;
                    if let Some(match_index) = node.match_index.get_mut(peer_id) {
                        *match_index = prev_log_index + entries_len as u64;
                    }
                    if let Some(next_index) = node.next_index.get_mut(peer_id) {
                        *next_index = prev_log_index + entries_len as u64 + 1;
                    }
                } else {
                    warn!("📋 节点 {} 拒绝日志同步，回退next_index", peer_id);
                    self.handle_log_mismatch(peer_id).await;
                }
            }
            Err(RaftClientError::LogIndexMismatch) => {
                warn!("📋 节点 {} 仍然不匹配，回退next_index", peer_id);
                self.handle_log_mismatch(peer_id).await;
            }
            Err(e) => {
                error!("❌ 向节点 {} 同步日志失败: {}", peer_id, e);
            }
        }
    }

    /// 提议配置更改（客户端接口）
    pub async fn propose_config(&self, key: String, value: Vec<u8>) -> Result<bool> {
        let node = self.node.lock().await;
        if node.role != NodeRole::Leader {
            return Err(anyhow::anyhow!("只有Leader可以提议配置更改"));
        }

        let entry = LogEntry {
            term: node.current_term,
            index: node.log.last_log_index() + 1,
            data: serialize_config_change(key.clone(), value),
            entry_type: "config".to_string(),
            key: key,
        };

        drop(node); // 释放读锁

        // 使用日志复制模块进行复制
        match self
            .log_replication
            .replicate_entry(self.node.clone(), entry)
            .await
        {
            Ok(ReplicationResult::Success) => {
                println!("日志条目复制成功");
                Ok(true)
            }
            Ok(ReplicationResult::Failed(msg)) => {
                println!("日志条目复制失败: {}", msg);
                Ok(false)
            }
            Ok(ReplicationResult::InProgress) => {
                println!("日志复制仍在进行中");
                Ok(false)
            }
            Ok(ReplicationResult::ConsistencyError) => {
                println!("日志一致性检查失败");
                Ok(false)
            }
            Err(e) => {
                println!("日志复制过程出错: {}", e);
                Err(e)
            }
        }
    }

    /// 获取节点ID
    pub async fn get_node_id(&self) -> String {
        let node = self.node.lock().await;
        node.node_id.clone()
    }

    /// 获取当前节点角色
    pub async fn get_role(&self) -> NodeRole {
        let node = self.node.lock().await;
        node.role
    }

    /// 获取当前任期
    pub async fn get_current_term(&self) -> u64 {
        let node = self.node.lock().await;
        node.current_term
    }

    /// 获取当前Leader ID
    pub async fn get_leader_id(&self) -> Option<String> {
        let node = self.node.lock().await;
        node.leader_id.clone()
    }

    /// 处理投票请求 - 深度集成方法
    pub async fn handle_vote_request(&self, req: &VoteRequest) -> VoteResponse {
        info!("🗳️  处理投票请求: candidate={}, term={}", req.candidate_id, req.term);
        
        let mut node = self.node.lock().await;
        
        // 1. 任期检查和更新
        if req.term > node.current_term {
            info!("📈 发现更高任期，更新: {} -> {}", node.current_term, req.term);
            node.current_term = req.term;
            node.voted_for = None;
            node.role = NodeRole::Follower;
            node.leader_id = None;
        }

        let mut vote_granted = false;
        
        // 2. 投票决策逻辑
        if req.term >= node.current_term {
            // 检查是否已经投票
            let can_vote = node.voted_for.is_none() || 
                          node.voted_for.as_ref() == Some(&req.candidate_id);
            
            // 检查候选人日志是否足够新
            let log_up_to_date = self.is_candidate_log_up_to_date(&node, req);
            
            if can_vote && log_up_to_date {
                vote_granted = true;
                node.voted_for = Some(req.candidate_id.clone());
                info!("✅ 投票给候选人: {}", req.candidate_id);
                
                // 重置选举超时
                self.reset_election_timeout(&mut node).await;
            } else {
                if !can_vote {
                    warn!("🚫 拒绝投票 - 已投票给: {:?}", node.voted_for);
                }
                if !log_up_to_date {
                    warn!("🚫 拒绝投票 - 候选人日志不够新");
                }
            }
        } else {
            warn!("🚫 拒绝投票 - 候选人任期过低: {} < {}", req.term, node.current_term);
        }

        VoteResponse {
            term: node.current_term,
            vote_granted,
            voter_id: node.node_id.clone(),
        }
    }

    /// 处理日志追加请求 - 深度集成方法
    pub async fn handle_append_entries(&self, req: &AppendEntriesRequest) -> AppendEntriesResponse {
        info!("📝 处理AppendEntries: leader={}, term={}, entries={}", 
              req.leader_id, req.term, req.entries.len());
        
        let mut node = self.node.lock().await;
        let mut success = false;
        let mut conflict_index = 0;

        // 1. 任期检查
        if req.term > node.current_term {
            info!("📈 发现更高任期，更新: {} -> {}", node.current_term, req.term);
            node.current_term = req.term;
            node.voted_for = None;
            node.role = NodeRole::Follower;
            node.leader_id = Some(req.leader_id.clone());
        } else if req.term < node.current_term {
            warn!("🚫 拒绝AppendEntries - Leader任期过低: {} < {}", req.term, node.current_term);
            return AppendEntriesResponse {
                term: node.current_term,
                success: false,
                follower_id: node.node_id.clone(),
                conflict_index: 0,
            };
        }

        // 2. 确认Leader身份
        if node.role != NodeRole::Follower {
            info!("🔄 转换为Follower角色");
            node.role = NodeRole::Follower;
        }
        node.leader_id = Some(req.leader_id.clone());

        // 3. 日志一致性检查
        if self.check_log_consistency(&node, req) {
            success = true;
            
            // 4. 处理日志条目
            if !req.entries.is_empty() {
                info!("📋 添加 {} 个日志条目", req.entries.len());
                self.append_log_entries(&mut node, req).await;
            }

            // 5. 更新commit_index
            if req.leader_commit > node.log.commit_index {
                let new_commit_index = req.leader_commit.min(node.log.last_log_index());
                info!("📤 更新commit_index: {} -> {}", node.log.commit_index, new_commit_index);
                node.log.commit_index = new_commit_index;
            }

            info!("💗 重置选举超时 - 收到有效Leader消息");
            self.reset_election_timeout(&mut node).await;
        } else {
            warn!("🔍 日志一致性检查失败");
            success = false;
            conflict_index = self.find_conflict_index(&node, req);
        }

        AppendEntriesResponse {
            term: node.current_term,
            success,
            follower_id: node.node_id.clone(),
            conflict_index,
        }
    }

    /// 从状态机读取配置
    pub async fn read_config_from_state_machine(&self, key: &str) -> Result<(Vec<u8>, u64), String> {
        let node = self.node.lock().await;
        
        info!("🔍 从状态机读取配置: key={}", key);
        
        // 访问状态机配置
        if let Some(value) = node.state_machine.config.get(key) {
            Ok((value.as_bytes().to_vec(), node.current_term))
        } else {
            Err(format!("配置项不存在: {}", key))
        }
    }

    /// 获取集群状态信息
    pub async fn get_cluster_info(&self) -> ClusterInfo {
        let node = self.node.lock().await;
        
        ClusterInfo {
            node_id: node.node_id.clone(),
            current_term: node.current_term,
            role: node.role,
            leader_id: node.leader_id.clone(),
            peers: node.peers.clone(),
            last_log_index: node.log.last_log_index(),
            commit_index: node.log.commit_index,
        }
    }

    // === 私有辅助方法 ===

    /// 检查候选人日志是否足够新
    fn is_candidate_log_up_to_date(&self, node: &RaftNode, req: &VoteRequest) -> bool {
        let last_log_index = node.log.last_log_index();
        let last_log_term = node.log.last_log_term();

        // Raft论文5.4.1: 比较最后日志条目的任期和索引
        if req.last_log_term > last_log_term {
            return true;
        }
        if req.last_log_term < last_log_term {
            return false;
        }
        // 任期相同，比较索引
        req.last_log_index >= last_log_index
    }

    /// 检查日志一致性
    fn check_log_consistency(&self, node: &RaftNode, req: &AppendEntriesRequest) -> bool {
        // 如果prev_log_index为0，总是匹配（初始状态）
        if req.prev_log_index == 0 {
            return true;
        }

        // 检查在prev_log_index位置是否有日志条目
        if req.prev_log_index > node.log.last_log_index() {
            return false;
        }

        // 检查任期是否匹配
        if let Some(term) = node.log.get_term_at(req.prev_log_index) {
            term == req.prev_log_term
        } else {
            false
        }
    }

    /// 添加日志条目
    async fn append_log_entries(&self, node: &mut RaftNode, req: &AppendEntriesRequest) {
        // 如果存在冲突的日志条目，删除它们
        let start_index = req.prev_log_index + 1;
        
        // 检查是否有冲突
        for (i, entry) in req.entries.iter().enumerate() {
            let entry_index = start_index + i as u64;
            if let Some(existing_entry) = node.log.get_entry_at(entry_index) {
                if existing_entry.term != entry.term {
                    // 发现冲突，删除从这个位置开始的所有日志
                    node.log.truncate_from(entry_index);
                    break;
                }
            }
        }

        // 添加新的日志条目
        for entry in &req.entries {
            node.log.append_entry(entry.clone());
        }
    }

    /// 查找冲突索引
    fn find_conflict_index(&self, node: &RaftNode, req: &AppendEntriesRequest) -> u64 {
        // 简化实现：返回我们认为应该开始同步的索引
        if req.prev_log_index > node.log.last_log_index() {
            node.log.last_log_index()
        } else {
            req.prev_log_index
        }
    }

    /// 重置选举超时
    async fn reset_election_timeout(&self, node: &mut RaftNode) {
        use rand::Rng;
        let timeout_ms = rand::thread_rng().gen_range(150..=300);
        node.election_timeout = Instant::now() + Duration::from_millis(timeout_ms);
    }
}

/// 集群信息结构
pub struct ClusterInfo {
    pub node_id: String,
    pub current_term: u64,
    pub role: NodeRole,
    pub leader_id: Option<String>,
    pub peers: Vec<String>,
    pub last_log_index: u64,
    pub commit_index: u64,
}

impl Clone for RaftEngine {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            leader_election: self.leader_election.clone(),
            log_replication: self.log_replication.clone(),
            client: self.client.clone(),
            running: self.running.clone(),
        }
    }
}

fn serialize_config_change(key: String, value: Vec<u8>) -> Vec<u8> {
    // 序列化配置更改为 Key:Value 格式
    let mut buf = Vec::new();
    buf.extend_from_slice(key.as_bytes());
    buf.extend_from_slice(b":");
    buf.extend_from_slice(&value);
    buf
}

#[cfg(test)]
mod deep_integration_tests {
    use super::*;
    use crate::pb::{VoteRequest, AppendEntriesRequest, LogEntry};
    use tokio;

    /// 创建测试用的RaftEngine
    async fn create_test_engine() -> RaftEngine {
        let mut node = RaftNode {
            node_id: "test-node".to_string(),
            current_term: 1,
            voted_for: None,
            log: RaftLog::new(),
            role: NodeRole::Follower,
            leader_id: None,
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            state_machine: ConfigStateMachine::new(),
            peers: vec!["peer1".to_string(), "peer2".to_string()],
            heartbeat_timeout: Instant::now(),
            election_timeout: Instant::now(),
        };
        
        // 添加一些测试配置到状态机
        node.state_machine.config.insert("test_key".to_string(), "test_value".to_string());
        
        let client = RaftClient::new();
        RaftEngine::new(node, client)
    }

    #[tokio::test]
    async fn test_handle_vote_request_success() {
        let engine = create_test_engine().await;
        
        let vote_req = VoteRequest {
            term: 2, // 更高的任期
            candidate_id: "candidate-1".to_string(),
            last_log_index: 0,
            last_log_term: 0,
        };

        let response = engine.handle_vote_request(&vote_req).await;

        assert_eq!(response.term, 2);
        assert!(response.vote_granted);
        assert_eq!(response.voter_id, "test-node");

        // 验证状态更新
        let node = engine.node.lock().await;
        assert_eq!(node.current_term, 2);
        assert_eq!(node.voted_for, Some("candidate-1".to_string()));
        assert_eq!(node.role, NodeRole::Follower);
    }

    #[tokio::test]
    async fn test_handle_vote_request_reject_lower_term() {
        let engine = create_test_engine().await;
        
        let vote_req = VoteRequest {
            term: 0, // 更低的任期
            candidate_id: "candidate-1".to_string(),
            last_log_index: 0,
            last_log_term: 0,
        };

        let response = engine.handle_vote_request(&vote_req).await;

        assert_eq!(response.term, 1);
        assert!(!response.vote_granted);
        assert_eq!(response.voter_id, "test-node");
    }

    #[tokio::test]
    async fn test_handle_append_entries_success() {
        let engine = create_test_engine().await;
        
        let append_req = AppendEntriesRequest {
            term: 2, // 更高的任期
            leader_id: "leader-1".to_string(),
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![LogEntry {
                term: 2,
                index: 1,
                data: b"test_data".to_vec(),
                entry_type: "config_set".to_string(),
                key: "test_key".to_string(),
            }],
            leader_commit: 0,
        };

        let response = engine.handle_append_entries(&append_req).await;

        assert_eq!(response.term, 2);
        assert!(response.success);
        assert_eq!(response.follower_id, "test-node");

        // 验证状态更新
        let node = engine.node.lock().await;
        assert_eq!(node.current_term, 2);
        assert_eq!(node.role, NodeRole::Follower);
        assert_eq!(node.leader_id, Some("leader-1".to_string()));
        assert_eq!(node.log.last_log_index(), 1);
    }

    #[tokio::test]
    async fn test_handle_append_entries_reject_lower_term() {
        let engine = create_test_engine().await;
        
        let append_req = AppendEntriesRequest {
            term: 0, // 更低的任期
            leader_id: "leader-1".to_string(),
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit: 0,
        };

        let response = engine.handle_append_entries(&append_req).await;

        assert_eq!(response.term, 1);
        assert!(!response.success);
        assert_eq!(response.follower_id, "test-node");
    }

    #[tokio::test]
    async fn test_read_config_from_state_machine() {
        let engine = create_test_engine().await;
        
        let result = engine.read_config_from_state_machine("test_key").await;
        
        assert!(result.is_ok());
        let (value, term) = result.unwrap();
        assert_eq!(value, b"test_value");
        assert_eq!(term, 1);
    }

    #[tokio::test]
    async fn test_get_cluster_info() {
        let engine = create_test_engine().await;
        
        let info = engine.get_cluster_info().await;
        
        assert_eq!(info.node_id, "test-node");
        assert_eq!(info.current_term, 1);
        assert_eq!(info.role, NodeRole::Follower);
        assert_eq!(info.peers, vec!["peer1", "peer2"]);
        assert_eq!(info.last_log_index, 0);
        assert_eq!(info.commit_index, 0);
    }

    #[tokio::test]
    async fn test_deep_integration_workflow() {
        let engine = create_test_engine().await;
        
        // 1. 处理投票请求
        let vote_req = VoteRequest {
            term: 2,
            candidate_id: "candidate-1".to_string(),
            last_log_index: 0,
            last_log_term: 0,
        };
        let vote_response = engine.handle_vote_request(&vote_req).await;
        assert!(vote_response.vote_granted);
        
        // 2. 处理AppendEntries请求
        let append_req = AppendEntriesRequest {
            term: 3, // 更高的任期
            leader_id: "leader-1".to_string(),
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![LogEntry {
                term: 3,
                index: 1,
                data: b"config_update".to_vec(),
                entry_type: "config_set".to_string(),
                key: "config_key".to_string(),
            }],
            leader_commit: 1,
        };
        let append_response = engine.handle_append_entries(&append_req).await;
        assert!(append_response.success);
        
        // 3. 检查最终状态
        let info = engine.get_cluster_info().await;
        assert_eq!(info.current_term, 3);
        assert_eq!(info.leader_id, Some("leader-1".to_string()));
        assert_eq!(info.last_log_index, 1);
        assert_eq!(info.commit_index, 1);
    }
}
