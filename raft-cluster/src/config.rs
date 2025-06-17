use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// 集群配置 - 类似 Hadoop 的 Configuration 类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub cluster: ClusterInfo,
    pub nodes: Vec<NodeConfig>,
    pub raft: RaftConfig,
    pub storage: StorageConfig,
    pub network: NetworkConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub grpc_port: u16,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    pub election_timeout_min: u64,
    pub election_timeout_max: u64,
    pub heartbeat_interval: u64,
    pub log_compaction: LogCompactionConfig,
    pub client_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogCompactionConfig {
    pub max_log_entries: usize,
    pub snapshot_threshold: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub persistence: PersistenceConfig,
    pub log: LogStorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceConfig {
    pub enabled: bool,
    pub sync_on_write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStorageConfig {
    pub max_size_mb: usize,
    pub rotation_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub connect_timeout: u64,
    pub read_timeout: u64,
    pub write_timeout: u64,
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub backoff_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub output: String,
    pub file: FileLoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLoggingConfig {
    pub enabled: bool,
    pub path: String,
    pub max_size_mb: usize,
    pub max_files: usize,
}

/// 配置加载器 - 类似 Hadoop 的 Configuration.addResource()
pub struct ConfigLoader;

impl ConfigLoader {
    /// 从YAML文件加载配置
    pub fn load_from_yaml<P: AsRef<Path>>(path: P) -> Result<ClusterConfig> {
        let path = path.as_ref();
        info!("📁 正在加载集群配置文件: {}", path.display());

        if !path.exists() {
            return Err(anyhow!("配置文件不存在: {}", path.display()));
        }

        let content = fs::read_to_string(path)
            .map_err(|e| anyhow!("读取配置文件失败: {}", e))?;

        let config: ClusterConfig = serde_yaml::from_str(&content)
            .map_err(|e| anyhow!("解析YAML配置失败: {}", e))?;

        Self::validate_config(&config)?;
        
        info!("✅ 集群配置加载成功: {} (版本: {})", 
              config.cluster.name, config.cluster.version);
        info!("🌐 发现 {} 个节点", config.nodes.len());
        
        Ok(config)
    }

    /// 从环境变量加载配置 (类似 Hadoop 的环境变量覆盖)
    pub fn load_from_env(base_config: &mut ClusterConfig) -> Result<()> {
        info!("🔧 检查环境变量配置覆盖...");

        // 覆盖集群名称
        if let Ok(cluster_name) = std::env::var("RAFT_CLUSTER_NAME") {
            warn!("🔄 环境变量覆盖集群名称: {} -> {}", 
                  base_config.cluster.name, cluster_name);
            base_config.cluster.name = cluster_name;
        }

        // 覆盖日志级别
        if let Ok(log_level) = std::env::var("RAFT_LOG_LEVEL") {
            warn!("🔄 环境变量覆盖日志级别: {} -> {}", 
                  base_config.logging.level, log_level);
            base_config.logging.level = log_level;
        }

        // 覆盖选举超时
        if let Ok(timeout_str) = std::env::var("RAFT_ELECTION_TIMEOUT_MIN") {
            if let Ok(timeout) = timeout_str.parse::<u64>() {
                warn!("🔄 环境变量覆盖选举超时最小值: {} -> {}", 
                      base_config.raft.election_timeout_min, timeout);
                base_config.raft.election_timeout_min = timeout;
            }
        }

        if let Ok(timeout_str) = std::env::var("RAFT_ELECTION_TIMEOUT_MAX") {
            if let Ok(timeout) = timeout_str.parse::<u64>() {
                warn!("🔄 环境变量覆盖选举超时最大值: {} -> {}", 
                      base_config.raft.election_timeout_max, timeout);
                base_config.raft.election_timeout_max = timeout;
            }
        }

        info!("✅ 环境变量配置加载完成");
        Ok(())
    }

    /// 验证配置合法性
    fn validate_config(config: &ClusterConfig) -> Result<()> {
        info!("🔍 验证集群配置...");

        // 验证节点配置
        if config.nodes.is_empty() {
            return Err(anyhow!("集群必须至少包含一个节点"));
        }

        // 验证节点ID唯一性
        let mut node_ids = std::collections::HashSet::new();
        for node in &config.nodes {
            if !node_ids.insert(&node.id) {
                return Err(anyhow!("发现重复的节点ID: {}", node.id));
            }
        }

        // 验证端口唯一性
        let mut ports = std::collections::HashSet::new();
        for node in &config.nodes {
            let address = format!("{}:{}", node.host, node.port);
            if !ports.insert(address.clone()) {
                return Err(anyhow!("发现重复的节点地址: {}", address));
            }
        }

        // 验证Raft配置
        if config.raft.election_timeout_min >= config.raft.election_timeout_max {
            return Err(anyhow!("选举超时最小值必须小于最大值"));
        }

        if config.raft.heartbeat_interval >= config.raft.election_timeout_min {
            return Err(anyhow!("心跳间隔必须小于选举超时最小值"));
        }

        info!("✅ 配置验证通过");
        Ok(())
    }
}

impl ClusterConfig {
    /// 获取当前节点配置
    pub fn get_node_config(&self, node_id: &str) -> Option<&NodeConfig> {
        self.nodes.iter().find(|node| node.id == node_id)
    }

    /// 获取其他节点列表 (类似 Hadoop 的 getSlaves())
    pub fn get_peer_nodes(&self, current_node_id: &str) -> Vec<&NodeConfig> {
        self.nodes.iter()
            .filter(|node| node.id != current_node_id)
            .collect()
    }

    /// 获取所有节点的gRPC地址
    pub fn get_all_grpc_addresses(&self) -> Vec<String> {
        self.nodes.iter()
            .map(|node| format!("http://{}:{}", node.host, node.grpc_port))
            .collect()
    }

    /// 获取peer节点的gRPC地址
    pub fn get_peer_grpc_addresses(&self, current_node_id: &str) -> Vec<String> {
        self.get_peer_nodes(current_node_id)
            .into_iter()
            .map(|node| format!("http://{}:{}", node.host, node.grpc_port))
            .collect()
    }

    /// 创建数据目录 (类似 Hadoop 的数据目录初始化)
    pub fn ensure_data_directories(&self) -> Result<()> {
        info!("📁 确保所有数据目录存在...");
        
        for node in &self.nodes {
            let data_dir = Path::new(&node.data_dir);
            if !data_dir.exists() {
                fs::create_dir_all(data_dir)
                    .map_err(|e| anyhow!("创建数据目录失败 {}: {}", data_dir.display(), e))?;
                info!("📁 创建数据目录: {}", data_dir.display());
            }
        }

        info!("✅ 所有数据目录检查完成");
        Ok(())
    }
} 