use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从命令行参数获取要监听的文件名
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("用法: {} <config_file_name>", args[0]);
        eprintln!("示例: {} app.yaml", args[0]);
        std::process::exit(1);
    }
    
    let file_name = &args[1];
    let url = format!("ws://127.0.0.1:8080/ws/listen?file={}", file_name);
    
    println!("🔌 连接到 WebSocket: {}", url);
    
    // 连接到 WebSocket 服务器
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    
    println!("✅ WebSocket 连接成功！");
    println!("🔄 开始监听配置文件: {}", file_name);
    println!("📝 输入 'ping' 测试连接，输入 'quit' 退出\n");
    
    // 启动消息接收任务
    let read_task = tokio::spawn(async move {
        while let Some(message) = read.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    // 尝试解析 JSON 消息
                    match serde_json::from_str::<Value>(&text) {
                        Ok(json) => {
                            let msg_type = json["type"].as_str().unwrap_or("unknown");
                            match msg_type {
                                "initial" => {
                                    println!("📄 收到初始配置:");
                                    if let Some(config) = json["config"].as_object() {
                                        println!("   {}", serde_json::to_string_pretty(config)?);
                                    }
                                }
                                "update" => {
                                    println!("🔄 配置文件已更新！");
                                    println!("   文件: {}", json["file"].as_str().unwrap_or("unknown"));
                                    println!("   时间: {}", json["timestamp"].as_str().unwrap_or("unknown"));
                                    if let Some(config) = json["config"].as_str() {
                                        println!("   新配置: {}", config);
                                    }
                                }
                                "pong" => {
                                    println!("🏓 收到 pong: {}", json["timestamp"].as_str().unwrap_or("unknown"));
                                }
                                "error" => {
                                    println!("❌ 错误: {}", json["message"].as_str().unwrap_or("unknown"));
                                }
                                _ => {
                                    println!("📨 收到消息: {}", text);
                                }
                            }
                        }
                        Err(_) => {
                            println!("📨 收到原始消息: {}", text);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    println!("🔌 服务器关闭了连接");
                    break;
                }
                Err(e) => {
                    println!("❌ 接收消息时出错: {}", e);
                    break;
                }
                _ => {}
            }
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });
    
    // 启动用户输入处理任务
    let input_task = tokio::spawn(async move {
        let stdin = io::stdin();
        loop {
            print!("ws-client> ");
            io::stdout().flush().unwrap();
            
            let mut input = String::new();
            match stdin.read_line(&mut input) {
                Ok(0) => {
                    println!("\n👋 检测到EOF，退出...");
                    break;
                }
                Ok(_) => {
                    let command = input.trim();
                    
                    if command.is_empty() {
                        continue;
                    }
                    
                    if command == "quit" || command == "exit" {
                        println!("👋 再见！");
                        break;
                    }
                    
                    // 发送消息到服务器
                    if let Err(e) = write.send(Message::Text(command.to_string())).await {
                        println!("❌ 发送消息失败: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    println!("❌ 读取输入时出错: {}", e);
                    break;
                }
            }
        }
        
        // 发送关闭消息
        let _ = write.close().await;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });
    
    // 等待任务完成
    tokio::select! {
        result = read_task => {
            if let Err(e) = result? {
                println!("读取任务错误: {}", e);
            }
        }
        result = input_task => {
            if let Err(e) = result? {
                println!("输入任务错误: {}", e);
            }
        }
    }
    
    println!("👋 WebSocket 客户端已退出");
    Ok(())
} 