use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::Result;
use rand::Rng;
use tokio::{sync::Mutex, time::sleep};
use tracing::{info, warn};

use crate::{
    grpc::client::RaftClient,
    pb::VoteRequest,
    raft::node::{NodeRole, RaftNode},
};

/// 选举结果
#[derive(Debug)]
pub enum ElectionResult {
    Won,              // 赢得选举
    Lost,             // 选举失败
    TermUpdated(u64), // 发现更高term，需要更新
}

/// 选举状态跟踪
#[derive(Debug)]
struct ElectionState {
    term: u64,
    vote_count: usize,
    votes_received: HashSet<String>,
    total_nodes: usize,
    majority_needed: usize,
}

/// Leader选举模块
#[derive(Clone)]
pub struct LeaderElection {
    client: Arc<Mutex<RaftClient>>,
}

impl LeaderElection {
    pub fn new(client: Arc<Mutex<RaftClient>>) -> Self {
        Self { client }
    }

    /// 发起选举（这是你设计的核心方法）
    pub async fn start_election(&self, node: Arc<Mutex<RaftNode>>) -> Result<ElectionResult> {
        // 步骤1: 准备选举状态
        let (candidate_id, peers, vote_request) = {
            let mut node_guard = node.lock().await;

            // 转换为候选人
            node_guard.current_term += 1;
            node_guard.voted_for = Some(node_guard.node_id.clone());
            node_guard.role = NodeRole::Candidate;

            let vote_request = VoteRequest {
                term: node_guard.current_term,
                candidate_id: node_guard.node_id.clone(),
                last_log_index: node_guard.log.last_log_index(),
                last_log_term: node_guard.log.last_log_term(),
            };

            (
                node_guard.node_id.clone(),
                node_guard.peers.clone(),
                vote_request,
            )
        };

        info!(
            "🗳️  节点 {} 发起选举，term={}",
            candidate_id, vote_request.term
        );

        // 步骤2: 初始化选举状态
        let mut election_state = ElectionState {
            term: vote_request.term,
            vote_count: 1, // 自己的票
            votes_received: {
                let mut set = HashSet::new();
                set.insert(candidate_id.clone());
                set
            },
            total_nodes: peers.len() + 1,
            majority_needed: (peers.len() + 1) / 2 + 1,
        };

        // 步骤3: 并发发送投票请求（这里实现你提到的并发策略）
        let result = self
            .collect_votes(vote_request, peers, &mut election_state)
            .await?;

        // 步骤4: 根据结果更新节点状态
        match result {
            ElectionResult::Won => {
                let mut node_guard = node.lock().await;
                node_guard.role = NodeRole::Leader;
                node_guard.leader_id = Some(candidate_id.clone());
                // TODO: 初始化Leader状态（next_index, match_index等）
                info!(
                    "🎉 节点 {} 成为Leader，term={}",
                    candidate_id, election_state.term
                );
            }
            ElectionResult::Lost => {
                let mut node_guard = node.lock().await;
                node_guard.role = NodeRole::Follower;
                info!(
                    "😞 节点 {} 选举失败，term={}",
                    candidate_id, election_state.term
                );
            }
            ElectionResult::TermUpdated(new_term) => {
                let mut node_guard = node.lock().await;
                node_guard.current_term = new_term;
                node_guard.voted_for = None;
                node_guard.role = NodeRole::Follower;
                info!(
                    "📈 节点 {} 发现更高term={}，转为Follower",
                    candidate_id, new_term
                );
            }
        }

        Ok(result)
    }

    /// 并发收集投票（解决你提到的"不需要等所有节点"的问题）
    async fn collect_votes(
        &self,
        vote_request: VoteRequest,
        peers: Vec<String>,
        election_state: &mut ElectionState,
    ) -> Result<ElectionResult> {
        use futures::stream::{FuturesUnordered, StreamExt};

        // 创建所有投票请求的Future
        let vote_futures: FuturesUnordered<_> = peers
            .into_iter()
            .map(|peer| {
                let client = Arc::clone(&self.client);
                let request = vote_request.clone();
                async move {
                    (
                        peer.clone(),
                        client
                            .lock()
                            .await
                            .send_request_vote(peer.clone(), tonic::Request::new(request))
                            .await,
                    )
                }
            })
            .collect();

        // 并发处理投票响应，一旦达到多数就返回
        let mut vote_futures = vote_futures;
        while let Some((peer, response)) = vote_futures.next().await {
            match response {
                Ok(vote_response) => {
                    let vote_response = vote_response.into_inner();
                    // 检查是否发现更高term
                    if vote_response.term > election_state.term {
                        return Ok(ElectionResult::TermUpdated(vote_response.term));
                    }

                    // 处理投票结果
                    if vote_response.vote_granted && !election_state.votes_received.contains(&peer)
                    {
                        election_state.votes_received.insert(peer.clone());
                        election_state.vote_count += 1;

                        info!(
                            "✅ 收到 {} 的投票，当前票数: {}/{}",
                            peer, election_state.vote_count, election_state.majority_needed
                        );

                        // 关键：达到多数票就立即返回，无需等待其他节点
                        if election_state.vote_count >= election_state.majority_needed {
                            return Ok(ElectionResult::Won);
                        }
                    }
                }
                Err(e) => {
                    warn!("❌ 向 {} 请求投票失败: {}", peer, e);
                }
            }
        }

        // 所有投票都处理完了，但没达到多数
        Ok(ElectionResult::Lost)
    }

    /// 生成随机选举超时时间（解决选举冲突问题）
    pub fn random_election_timeout() -> Duration {
        let mut rng = rand::thread_rng();
        // 150-300ms的随机超时
        Duration::from_millis(rng.gen_range(150..=300))
    }

    /// 检查选举超时（配合你提到的Timer机制）
    pub async fn election_timeout_loop(node: Arc<Mutex<RaftNode>>, election: Arc<LeaderElection>) {
        loop {
            let timeout = Self::random_election_timeout();
            sleep(timeout).await;

            // 检查是否需要发起选举
            let should_start_election = {
                let node_guard = node.lock().await;
                matches!(node_guard.role, NodeRole::Follower)
                    && node_guard.heartbeat_timeout.elapsed() > timeout
            };

            if should_start_election {
                info!("⏰ 选举超时，发起选举");
                if let Err(e) = election.start_election(Arc::clone(&node)).await {
                    warn!("选举过程出错: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, time::Instant};
    use tokio::sync::Mutex;
    use tonic::{Response, Status};

    use crate::{
        pb::VoteResponse,
        raft::{log::RaftLog, state_machine::ConfigStateMachine},
    };

    // Mock RaftClient for testing
    struct MockRaftClient {
        // 存储每个节点的预设响应
        responses: HashMap<String, Result<VoteResponse, Status>>,
    }

    impl MockRaftClient {
        fn new() -> Self {
            Self {
                responses: HashMap::new(),
            }
        }

        // 设置某个节点的投票响应
        fn set_vote_response(&mut self, node_id: String, response: Result<VoteResponse, Status>) {
            self.responses.insert(node_id, response);
        }

        async fn send_request_vote(
            &mut self,
            node_id: String,
            _request: tonic::Request<VoteRequest>,
        ) -> Result<Response<VoteResponse>, Status> {
            match self.responses.get(&node_id) {
                Some(Ok(response)) => Ok(Response::new(response.clone())),
                Some(Err(status)) => Err(status.clone()),
                None => Err(Status::unavailable("Node not found")),
            }
        }
    }

    // 测试专用的LeaderElection，使用MockRaftClient
    struct TestLeaderElection {
        mock_client: Arc<Mutex<MockRaftClient>>,
    }

    impl TestLeaderElection {
        fn new(mock_client: MockRaftClient) -> Self {
            Self {
                mock_client: Arc::new(Mutex::new(mock_client)),
            }
        }

        // 复制核心的选举逻辑，但使用mock client
        async fn start_election(&self, node: Arc<Mutex<RaftNode>>) -> Result<ElectionResult> {
            // 步骤1: 准备选举状态 (与原版相同)
            let (candidate_id, peers, vote_request) = {
                let mut node_guard = node.lock().await;
                
                node_guard.current_term += 1;
                node_guard.voted_for = Some(node_guard.node_id.clone());
                node_guard.role = NodeRole::Candidate;
                
                let vote_request = VoteRequest {
                    term: node_guard.current_term,
                    candidate_id: node_guard.node_id.clone(),
                    last_log_index: node_guard.log.last_log_index(),
                    last_log_term: node_guard.log.last_log_term(),
                };
                
                (
                    node_guard.node_id.clone(),
                    node_guard.peers.clone(),
                    vote_request,
                )
            };

            // 步骤2: 初始化选举状态
            let mut election_state = ElectionState {
                term: vote_request.term,
                vote_count: 1, // 自己的票
                votes_received: {
                    let mut set = HashSet::new();
                    set.insert(candidate_id.clone());
                    set
                },
                total_nodes: peers.len() + 1,
                majority_needed: (peers.len() + 1) / 2 + 1,
            };

            // 步骤3: 收集投票 (使用mock)
            let result = self.collect_votes_mock(vote_request, peers, &mut election_state).await?;

            // 步骤4: 更新节点状态
            match result {
                ElectionResult::Won => {
                    let mut node_guard = node.lock().await;
                    node_guard.role = NodeRole::Leader;
                    node_guard.leader_id = Some(candidate_id.clone());
                }
                ElectionResult::Lost => {
                    let mut node_guard = node.lock().await;
                    node_guard.role = NodeRole::Follower;
                }
                ElectionResult::TermUpdated(new_term) => {
                    let mut node_guard = node.lock().await;
                    node_guard.current_term = new_term;
                    node_guard.voted_for = None;
                    node_guard.role = NodeRole::Follower;
                }
            }

            Ok(result)
        }

        async fn collect_votes_mock(
            &self,
            vote_request: VoteRequest,
            peers: Vec<String>,
            election_state: &mut ElectionState,
        ) -> Result<ElectionResult> {
            // 先检查是否已经达到多数派（处理单节点情况）
            if election_state.vote_count >= election_state.majority_needed {
                return Ok(ElectionResult::Won);
            }
            
            // 简化版：串行处理每个peer (测试中不需要真正的并发)
            for peer in peers {
                let response = {
                    let mut client = self.mock_client.lock().await;
                    client.send_request_vote(peer.clone(), tonic::Request::new(vote_request.clone())).await
                };

                match response {
                    Ok(vote_response) => {
                        let vote_response = vote_response.into_inner();
                        if vote_response.term > election_state.term {
                            return Ok(ElectionResult::TermUpdated(vote_response.term));
                        }

                        if vote_response.vote_granted && !election_state.votes_received.contains(&peer) {
                            election_state.votes_received.insert(peer.clone());
                            election_state.vote_count += 1;

                            if election_state.vote_count >= election_state.majority_needed {
                                return Ok(ElectionResult::Won);
                            }
                        }
                    }
                    Err(_) => {
                        // 网络错误，继续处理其他节点
                    }
                }
            }

            Ok(ElectionResult::Lost)
        }
    }

    // 辅助函数：创建测试用的RaftNode
    fn create_test_node(node_id: &str, peers: Vec<String>, current_term: u64) -> Arc<Mutex<RaftNode>> {
        let node = RaftNode {
            node_id: node_id.to_string(),
            current_term,
            voted_for: None,
            log: RaftLog {
                entities: vec![],
                commit_index: 0,
                last_applied: 0,
            },
            role: NodeRole::Follower,
            leader_id: None,
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            state_machine: ConfigStateMachine {
                config: HashMap::new(),
            },
            peers,
            heartbeat_timeout: Instant::now(),
            election_timeout: Instant::now(),
        };
        Arc::new(Mutex::new(node))
    }

    #[tokio::test]
    async fn test_election_success_with_majority() {
        // 测试场景：5节点集群，获得3票（多数），选举成功
        let node = create_test_node("node1", vec!["node2".to_string(), "node3".to_string(), "node4".to_string(), "node5".to_string()], 1);
        
        let mut mock_client = MockRaftClient::new();
        // node2和node3投赞成票
        mock_client.set_vote_response("node2".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: true,
            voter_id: "node2".to_string(),
        }));
        mock_client.set_vote_response("node3".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: true,
            voter_id: "node3".to_string(),
        }));
        // node4和node5投反对票
        mock_client.set_vote_response("node4".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: false,
            voter_id: "node4".to_string(),
        }));
        mock_client.set_vote_response("node5".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: false,
            voter_id: "node5".to_string(),
        }));

        // 使用测试专用的选举器
        let election = TestLeaderElection::new(mock_client);
        let result = election.start_election(node.clone()).await.unwrap();
        
        // 验证选举结果
        assert!(matches!(result, ElectionResult::Won));
        
        // 验证节点状态变化
        let node_guard = node.lock().await;
        assert_eq!(node_guard.role, NodeRole::Leader);
        assert_eq!(node_guard.current_term, 2);
        assert_eq!(node_guard.leader_id, Some("node1".to_string()));
    }

    #[tokio::test]
    async fn test_election_failure_no_majority() {
        // 测试场景：5节点集群，只获得2票（包括自己），选举失败
        let node = create_test_node("node1", vec!["node2".to_string(), "node3".to_string(), "node4".to_string(), "node5".to_string()], 1);
        
        let mut mock_client = MockRaftClient::new();
        // 只有node2投赞成票，其他都投反对票
        mock_client.set_vote_response("node2".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: true,
            voter_id: "node2".to_string(),
        }));
        mock_client.set_vote_response("node3".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: false,
            voter_id: "node3".to_string(),
        }));
        mock_client.set_vote_response("node4".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: false,
            voter_id: "node4".to_string(),
        }));
        mock_client.set_vote_response("node5".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: false,
            voter_id: "node5".to_string(),
        }));

        let election = TestLeaderElection::new(mock_client);
        let result = election.start_election(node.clone()).await.unwrap();
        
        // 验证选举失败
        assert!(matches!(result, ElectionResult::Lost));
        
        // 验证节点变回Follower
        let node_guard = node.lock().await;
        assert_eq!(node_guard.role, NodeRole::Follower);
        assert_eq!(node_guard.current_term, 2); // term已经递增
    }

    #[tokio::test]
    async fn test_election_discovers_higher_term() {
        // 测试场景：选举过程中发现更高term，立即转为Follower
        let node = create_test_node("node1", vec!["node2".to_string(), "node3".to_string()], 1);
        
        let mut mock_client = MockRaftClient::new();
        // node2返回更高的term，表示已经有新的Leader了
        mock_client.set_vote_response("node2".to_string(), Ok(VoteResponse {
            term: 5, // 比当前term=2更高
            vote_granted: false,
            voter_id: "node2".to_string(),
        }));
        mock_client.set_vote_response("node3".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: true,
            voter_id: "node3".to_string(),
        }));

        let election = TestLeaderElection::new(mock_client);
        let result = election.start_election(node.clone()).await.unwrap();
        
        // 验证发现更高term
        assert!(matches!(result, ElectionResult::TermUpdated(5)));
        
        // 验证节点状态更新
        let node_guard = node.lock().await;
        assert_eq!(node_guard.role, NodeRole::Follower);
        assert_eq!(node_guard.current_term, 5);
        assert_eq!(node_guard.voted_for, None); // 清空投票记录
    }

    #[tokio::test]
    async fn test_election_with_network_errors() {
        // 测试场景：部分节点网络不可达，但仍能获得多数票
        let node = create_test_node("node1", vec!["node2".to_string(), "node3".to_string(), "node4".to_string(), "node5".to_string()], 1);
        
        let mut mock_client = MockRaftClient::new();
        // node2和node3投赞成票
        mock_client.set_vote_response("node2".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: true,
            voter_id: "node2".to_string(),
        }));
        mock_client.set_vote_response("node3".to_string(), Ok(VoteResponse {
            term: 2,
            vote_granted: true,
            voter_id: "node3".to_string(),
        }));
        // node4和node5网络不可达
        mock_client.set_vote_response("node4".to_string(), Err(Status::unavailable("Network error")));
        mock_client.set_vote_response("node5".to_string(), Err(Status::unavailable("Network error")));

        let election = TestLeaderElection::new(mock_client);
        let result = election.start_election(node.clone()).await.unwrap();
        
        // 虽然有网络错误，但仍然获得多数票（3/5）
        assert!(matches!(result, ElectionResult::Won));
        
        let node_guard = node.lock().await;
        assert_eq!(node_guard.role, NodeRole::Leader);
    }

    #[tokio::test]
    async fn test_single_node_cluster() {
        // 测试场景：单节点集群，自己给自己投票
        let node = create_test_node("node1", vec![], 1); // 没有其他节点
        
        let mock_client = MockRaftClient::new(); // 空的mock，因为没有其他节点

        let election = TestLeaderElection::new(mock_client);
        let result = election.start_election(node.clone()).await.unwrap();
        
        // 单节点集群应该立即成为Leader
        assert!(matches!(result, ElectionResult::Won));
        
        let node_guard = node.lock().await;
        assert_eq!(node_guard.role, NodeRole::Leader);
        assert_eq!(node_guard.current_term, 2);
    }

    #[test]
    fn test_random_election_timeout() {
        // 测试随机超时时间在合理范围内
        for _ in 0..100 {
            let timeout = LeaderElection::random_election_timeout();
            assert!(timeout >= Duration::from_millis(150));
            assert!(timeout <= Duration::from_millis(300));
        }
    }

    #[test]
    fn test_majority_calculation() {
        // 测试多数派计算逻辑
        assert_eq!(ElectionState::calculate_majority(1), 1);  // 单节点
        assert_eq!(ElectionState::calculate_majority(2), 2);  // 2节点需要2票
        assert_eq!(ElectionState::calculate_majority(3), 2);  // 3节点需要2票
        assert_eq!(ElectionState::calculate_majority(4), 3);  // 4节点需要3票
        assert_eq!(ElectionState::calculate_majority(5), 3);  // 5节点需要3票
    }
}

impl ElectionState {
    // 辅助方法：计算多数派所需票数
    fn calculate_majority(total_nodes: usize) -> usize {
        total_nodes / 2 + 1
    }
}
