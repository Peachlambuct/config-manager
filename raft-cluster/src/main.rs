use anyhow::Result;
use clap::{Arg, Command};
use std::env;
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber;
use tokio::signal;

use crate::cluster::ClusterBootstrap;

mod config;
mod cluster;
mod grpc;
mod simple_raft;  // 添加simple_raft模块
mod storage;

// 引入生成的gRPC代码
pub mod pb {
    tonic::include_proto!("raft");
}

#[tokio::main]
async fn main() -> Result<()> {
    // 解析命令行参数 (类似 Hadoop 的启动脚本参数)
    let matches = Command::new("raft-cluster")
        .version("1.0.0")
        .about("Raft分布式共识集群节点")
        .author("Raft Team")
        .arg(
            Arg::new("node-id")
                .short('n')
                .long("node-id")
                .value_name("NODE_ID")
                .help("节点ID (如: node-1, node-2, node-3)")
                .required(false),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("CONFIG_FILE")
                .help("集群配置文件路径")
                .default_value("configs/cluster-config.yaml"),
        )
        .arg(
            Arg::new("demo")
                .short('d')
                .long("demo")
                .help("启动后演示Engine功能")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    // 获取配置文件路径
    let config_path = matches.get_one::<String>("config").unwrap();

    // 获取节点ID (支持命令行参数或环境变量)
    let node_id = if let Some(id) = matches.get_one::<String>("node-id") {
        id.clone()
    } else if let Ok(id) = env::var("RAFT_NODE_ID") {
        id
    } else {
        // 默认使用node-1 (适合开发环境)
        "node-1".to_string()
    };

    let demo_mode = matches.get_flag("demo");

    // 初始化日志系统
    init_logging().await?;

    info!("🚀 启动Raft集群节点");
    info!("📋 节点ID: {}", node_id);
    info!("📄 配置文件: {}", config_path);
    if demo_mode {
        info!("🎯 演示模式已启用");
    }

    // 创建和启动集群
    let mut bootstrap = ClusterBootstrap::new(config_path, node_id)?;

    // 启动集群 (在后台任务中)
    let startup_task = tokio::spawn(async move {
        if let Err(e) = bootstrap.start().await {
            error!("❌ 集群启动失败: {}", e);
            return Err(e);
        }

        // 如果启用了演示模式，展示Raft功能
        if demo_mode {
            info!("🎯 开始演示Raft功能...");
            if let Err(e) = bootstrap.demonstrate_raft_capabilities().await {
                error!("❌ Raft功能演示失败: {}", e);
            }
        }

        Ok(bootstrap)
    });

    tokio::select! {
        // 等待启动完成
        result = startup_task => {
            match result {
                Ok(Ok(bootstrap)) => {
                    info!("✅ 集群启动成功，等待关闭信号...");
                    
                    // 等待关闭信号
                    setup_graceful_shutdown().await;
                    
                    // 优雅关闭
                    info!("🛑 收到关闭信号，开始优雅停止...");
                    if let Err(e) = bootstrap.shutdown().await {
                        error!("❌ 优雅停止失败: {}", e);
                    }
                }
                Ok(Err(e)) => {
                    error!("❌ 集群启动失败: {}", e);
                    return Err(e);
                }
                Err(e) => {
                    error!("❌ 启动任务失败: {}", e);
                    return Err(e.into());
                }
            }
        }
        // 或者直接收到关闭信号
        _ = setup_graceful_shutdown() => {
            info!("🛑 收到关闭信号，正在启动中...");
        }
    }

    info!("👋 Raft集群节点已退出");
    Ok(())
}

/// 初始化日志系统
async fn init_logging() -> Result<()> {
    // 从环境变量获取日志级别，默认为INFO
    let log_level = env::var("RAFT_LOG_LEVEL")
        .unwrap_or_else(|_| "info".to_string());

    let level_filter = match log_level.to_lowercase().as_str() {
        "trace" => LevelFilter::TRACE,
        "debug" => LevelFilter::DEBUG,
        "info" => LevelFilter::INFO,
        "warn" => LevelFilter::WARN,
        "error" => LevelFilter::ERROR,
        _ => LevelFilter::INFO,
    };

    // 检查是否启用JSON格式日志
    let use_json = env::var("RAFT_LOG_FORMAT")
        .map(|f| f.to_lowercase() == "json")
        .unwrap_or(false);

    if use_json {
        // JSON格式暂不支持，使用标准格式
        tracing_subscriber::fmt()
            .with_max_level(level_filter)
            .with_target(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(level_filter)
            .with_target(false)
            .init();
    }

    info!("📝 日志系统初始化完成 (级别: {})", log_level);
    Ok(())
}

/// 设置优雅关闭信号处理
async fn setup_graceful_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        
        let mut sigint = signal(SignalKind::interrupt()).expect("创建SIGINT信号处理器失败");
        let mut sigterm = signal(SignalKind::terminate()).expect("创建SIGTERM信号处理器失败");
        
        tokio::select! {
            _ = sigint.recv() => {
                info!("🛑 收到SIGINT信号");
            }
            _ = sigterm.recv() => {
                info!("🛑 收到SIGTERM信号");
            }
        }
    }
    
    #[cfg(windows)]
    {
        let _ = signal::ctrl_c().await;
        info!("🛑 收到Ctrl+C信号");
    }
}
