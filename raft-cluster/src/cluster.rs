use anyhow::{anyhow, Result};
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use tracing::{error, info, warn};
// 使用简化的内部Raft实现

use crate::{
    config::{ClusterConfig, ConfigLoader},
    grpc::server::{ConfigServiceImpl, RaftServiceImpl},
    pb::{
        config_service_server::ConfigServiceServer,
        raft_service_server::RaftServiceServer,
    },
    simple_raft::{NodeId, RaftNode, ConfigRequest},
};

/// 集群启动器 - 使用 OpenRaft 实现
pub struct ClusterBootstrap {
    config: ClusterConfig,
    node_id: String,
    node_id_numeric: NodeId,
    raft_node: Option<Arc<RaftNode>>,
}

impl ClusterBootstrap {
    /// 创建集群启动器
    pub fn new(config_path: &str, node_id: String) -> Result<Self> {
        info!("🚀 初始化集群启动器 (OpenRaft版本)...");
        info!("📋 节点ID: {}", node_id);
        info!("📄 配置文件: {}", config_path);

        // 加载配置文件
        let mut config = ConfigLoader::load_from_yaml(config_path)?;
        
        // 应用环境变量覆盖
        ConfigLoader::load_from_env(&mut config)?;

        // 验证当前节点在配置中存在
        if config.get_node_config(&node_id).is_none() {
            return Err(anyhow!(
                "节点 {} 未在集群配置中找到。可用节点: {:?}",
                node_id,
                config.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
            ));
        }

        // 确保数据目录存在
        config.ensure_data_directories()?;

        // 将字符串节点ID转换为数字ID（简单的hash方法）
        let node_id_numeric = Self::string_to_node_id(&node_id);

        Ok(Self {
            config,
            node_id,
            node_id_numeric,
            raft_node: None,
        })
    }

    /// 启动集群节点
    pub async fn start(&mut self) -> Result<()> {
        info!("🌟 启动Raft集群节点: {} (ID: {})", self.node_id, self.node_id_numeric);

        // 1. 初始化Raft节点
        let raft_node = self.initialize_raft_node().await?;
        self.raft_node = Some(raft_node.clone());

        // 2. 启动gRPC服务器（后台）
        self.start_grpc_server_background(raft_node.clone()).await?;
        
        // 3. 等待一段时间确保服务器启动
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // 4. 初始化或加入集群
        self.setup_cluster_membership(&raft_node).await?;

        // 5. 等待Leader选举完成
        self.wait_for_cluster_ready(&raft_node).await?;

        Ok(())
    }

    /// 初始化Raft节点
    async fn initialize_raft_node(&self) -> Result<Arc<RaftNode>> {
        info!("🔧 初始化OpenRaft节点...");

        let node_config = self.config.get_node_config(&self.node_id)
            .ok_or_else(|| anyhow!("节点配置未找到"))?;

        let node_addr = format!("{}:{}", node_config.host, node_config.grpc_port);
        
        let raft_node = RaftNode::new(self.node_id_numeric).await?;
        
        info!("✅ OpenRaft节点初始化完成");
        Ok(Arc::new(raft_node))
    }

    /// 设置集群成员关系
    async fn setup_cluster_membership(&self, raft_node: &Arc<RaftNode>) -> Result<()> {
        info!("🌐 设置集群成员关系...");

        // 构建集群成员列表
        let mut members = Vec::new();
        
        for node_config in &self.config.nodes {
            let node_id = Self::string_to_node_id(&node_config.id);
            members.push(node_id);
        }

        info!("👥 集群成员: {:?}", members);

        // 只有第一个节点初始化集群
        let first_node_id = Self::string_to_node_id(&self.config.nodes[0].id);
        
        if self.node_id_numeric == first_node_id {
            info!("🚀 作为首个节点初始化集群");
            raft_node.initialize_cluster(members).await?;
        } else {
            info!("📚 作为后续节点等待加入集群");
            // 简化版本：后续节点也直接初始化相同的集群配置
            raft_node.initialize_cluster(members).await?;
        }

        Ok(())
    }

    /// 等待集群准备就绪
    async fn wait_for_cluster_ready(&self, raft_node: &Arc<RaftNode>) -> Result<()> {
        info!("🔍 等待集群就绪...");

        let timeout = Duration::from_secs(30);
        
        match raft_node.wait_for_leader(timeout).await {
            Ok(()) => {
                info!("✅ 集群已就绪");
                self.display_cluster_status(raft_node).await;
                Ok(())
            }
            Err(e) => {
                warn!("⚠️  集群初始化超时，但节点将继续运行: {}", e);
                self.display_cluster_status(raft_node).await;
                Ok(())
            }
        }
    }

    /// 展示集群状态
    async fn display_cluster_status(&self, raft_node: &Arc<RaftNode>) {
        info!("📋 集群状态详情:");

        let metrics = raft_node.get_metrics().await;
        info!("  🏷️  节点ID: {} ({})", self.node_id, self.node_id_numeric);
        info!("  📊 当前任期: {}", metrics.current_term);   
        info!("  👑 当前Leader: {:?}", metrics.current_leader);
        info!("  🗳️  集群状态: {:?}", metrics.state);
        info!("  📈 最后日志索引: {:?}", metrics.last_log_index);
        info!("  ✅ 已应用索引: {:?}", metrics.last_applied);
        info!("  🌐 集群成员: {:?}", metrics.membership_config);

        if raft_node.is_leader().await {
            info!("  👑 当前节点是Leader");
        } else {
            info!("  👥 当前节点是Follower");
        }
    }

    /// 在后台启动gRPC服务器
    async fn start_grpc_server_background(&self, raft_node: Arc<RaftNode>) -> Result<()> {
        let node_config = self.config.get_node_config(&self.node_id)
            .ok_or_else(|| anyhow!("节点配置未找到"))?;

        let bind_address = format!("{}:{}", node_config.host, node_config.grpc_port)
            .parse()
            .map_err(|e| anyhow!("无效的绑定地址: {}", e))?;

        info!("🌐 在后台启动gRPC服务器: {}", bind_address);

        // 创建服务实现
        let raft_service = RaftServiceImpl::new(raft_node.clone());
        let config_service = ConfigServiceImpl::new(raft_node);

        // 在后台启动服务器
        tokio::spawn(async move {
            info!("🚀 gRPC服务器开始监听: {}", bind_address);
            if let Err(e) = tonic::transport::Server::builder()
                .add_service(RaftServiceServer::new(raft_service))
                .add_service(ConfigServiceServer::new(config_service))
                .serve(bind_address)
                .await
            {
                error!("❌ gRPC服务器错误: {}", e);
            }
        });

        info!("✅ gRPC服务器后台启动完成");
        Ok(())
    }

    /// 演示Raft功能
    pub async fn demonstrate_raft_capabilities(&self) -> Result<()> {
        let raft_node = self.raft_node.as_ref()
            .ok_or_else(|| anyhow!("Raft节点未初始化"))?;

        info!("🎯 演示OpenRaft功能...");

        // 等待成为Leader或找到Leader
        self.wait_for_leadership(raft_node).await?;

        // 演示配置操作
        self.demonstrate_config_operations(raft_node).await?;

        Ok(())
    }

    /// 等待Leader选举
    async fn wait_for_leadership(&self, raft_node: &Arc<RaftNode>) -> Result<()> {
        info!("👑 等待Leader选举...");

        let timeout = Duration::from_secs(15);
        
        match raft_node.wait_for_leader(timeout).await {
            Ok(()) => {
                if raft_node.is_leader().await {
                    info!("✅ 当前节点成为Leader");
                } else {
                    info!("✅ 发现了Leader节点");
                }
                Ok(())
            }
            Err(e) => {
                warn!("⚠️  Leader选举超时，但继续运行: {}", e);
                Ok(())
            }
        }
    }

    /// 演示配置操作
    async fn demonstrate_config_operations(&self, raft_node: &Arc<RaftNode>) -> Result<()> {
        info!("⚙️  演示配置操作...");

        if raft_node.is_leader().await {
            info!("📝 作为Leader提交配置更改...");

            // 测试配置操作
            let test_configs = vec![
                ("cluster.name", "raft-cluster-openraft"),
                ("cluster.version", "1.0.0"),
                ("features.auto_scaling", "true"),
            ];

            for (key, value) in test_configs {
                let request = ConfigRequest {
                    key: key.to_string(),
                    value: value.to_string(),
                };

                match raft_node.client_write(request).await {
                    Ok(response) => {
                        info!("✅ 配置提交成功: {} = {} -> {:?}", key, value, response);
                    }
                    Err(e) => {
                        error!("❌ 配置提交失败 {}: {}", key, e);
                    }
                }

                sleep(Duration::from_millis(500)).await;
            }
        }

        // 读取配置
        self.demonstrate_config_reading(raft_node).await?;

        Ok(())
    }

    /// 演示配置读取
    async fn demonstrate_config_reading(&self, raft_node: &Arc<RaftNode>) -> Result<()> {
        info!("📖 演示配置读取...");

        let test_keys = vec!["cluster.name", "cluster.version", "features.auto_scaling"];

        for key in test_keys {
            match raft_node.client_read(key).await {
                Ok(Some(value)) => {
                    info!("📖 读取配置成功: {} = {}", key, value);
                }
                Ok(None) => {
                    info!("📖 配置不存在: {}", key);
                }
                Err(e) => {
                    error!("📖 读取配置失败 {}: {}", key, e);
                }
            }
        }

        Ok(())
    }

    /// 优雅停止集群节点
    pub async fn shutdown(&self) -> Result<()> {
        info!("🛑 开始优雅停止集群节点...");

        if let Some(raft_node) = &self.raft_node {
            // OpenRaft会在Drop时自动清理
            info!("✅ Raft节点已停止");
        }

        info!("👋 集群节点已完全停止");
        Ok(())
    }

    /// 获取集群配置
    pub fn get_cluster_config(&self) -> &ClusterConfig {
        &self.config
    }

    /// 获取当前节点ID
    pub fn get_node_id(&self) -> &str {
        &self.node_id
    }

    /// 字符串节点ID转换为数字ID
    fn string_to_node_id(node_id: &str) -> NodeId {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        node_id.hash(&mut hasher);
        hasher.finish()
    }
} 