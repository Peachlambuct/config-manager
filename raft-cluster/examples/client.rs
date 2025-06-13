use raft_cluster::grpc::client::RaftClient;
use raft_cluster::pb::VoteRequest;
use tonic::Request;

#[tokio::main]
async fn main() {
    let mut client = RaftClient::new();
    client
        .connect_to_node("node-1".to_string(), "http://127.0.0.1:50051".to_string())
        .await
        .unwrap();
    let request = Request::new(VoteRequest {
        term: 1,
        candidate_id: "node-2".to_string(),
        last_log_index: 0,
        last_log_term: 0,
    });
    client
        .send_request_vote("node-1".to_string(), request)
        .await
        .unwrap();
}
