use std::{collections::HashMap, time::Instant};

use crate::{
    pb::{VoteRequest, VoteResponse},
    raft::{log::RaftLog, state_machine::ConfigStateMachine},
};
use anyhow::Result;
use tonic::{Response, Status};

pub struct RaftNode {
    pub node_id: String, // 当前的节点ID
    pub current_term: u64, // 当前的任期
    pub voted_for: Option<String>, // 当前的投票者
    pub log: RaftLog, // 日志

    pub role: NodeRole, // 当前的节点角色
    pub leader_id: Option<String>, // 当前的leader节点

    pub next_index: HashMap<String, u64>, // 下一个要发送的日志索引
    pub match_index: HashMap<String, u64>, // 已经匹配的日志索引

    pub state_machine: ConfigStateMachine,  // 状态机

    pub peers: Vec<String>, // 集群中的所有节点

    pub heartbeat_timeout: Instant, // 心跳超时时间
    pub election_timeout: Instant, // 选举超时时间
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum NodeRole {
    Follower,
    Candidate,
    Leader,
}

impl RaftNode {
    async fn handle_vote_request(
        &mut self,
        request: VoteRequest,
    ) -> Result<Response<VoteResponse>, Status> {
        // 任期大于当前任期直接转为follower
        if request.term > self.current_term {
            self.current_term = request.term;
            self.voted_for = None;
            self.role = NodeRole::Follower;
        }

        let vote_granted = request.term >= self.current_term
            && (self.voted_for.is_none() || self.voted_for == Some(request.candidate_id.clone()))
            && self.is_log_up_to_date(&request); // 日志必须够新

        Ok(Response::new(VoteResponse {
            term: self.current_term,
            vote_granted,
            voter_id: self.node_id.clone(),
        }))
    }

    fn is_log_up_to_date(&self, request: &VoteRequest) -> bool {
        let last_log_index = self.log.last_log_index();
        let last_log_term = self.log.last_log_term();

        request.last_log_index >= last_log_index && request.last_log_term >= last_log_term
    }
}
