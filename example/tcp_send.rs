use std::io::Write;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:8080").await?;
    let mut reader = BufReader::new(stream);
    
    println!("🚀 已连接到配置管理服务器 127.0.0.1:8080");
    loop {
        let mut input = String::new();
        print!("config-cli> ");
        std::io::stdout().flush().unwrap();
        
        // 读取用户输入
        match std::io::stdin().read_line(&mut input) {
            Ok(0) => {
                println!("\n👋 检测到EOF，退出...");
                break;
            }
            Ok(_) => {
                let command = input.trim();
                
                // 检查空输入
                if command.is_empty() {
                    continue;
                }
                
                // 检查退出命令
                if command == "quit" || command == "exit" {
                    println!("👋 再见！");
                    break;
                }
                
                // 发送数据到服务器
                let stream = reader.get_mut();
                if let Err(e) = stream.write_all(input.as_bytes()).await {
                    println!("❌ 发送数据失败: {}", e);
                    break;
                }
                if let Err(e) = stream.flush().await {
                    println!("❌ 刷新流失败: {}", e);
                    break;
                }

                // 读取服务器响应
                let mut response = String::new();
                match reader.read_line(&mut response).await {
                    Ok(0) => {
                        println!("🔌 服务器关闭了连接");
                        break;
                    }
                    Ok(_) => {
                        let response = response.trim();
                        if response.starts_with("无效的命令") {
                            println!("⚠️  {}", response);
                            println!("💡 输入 'help' 查看可用命令");
                        } else {
                            println!("✅ {}", response);
                        }
                    }
                    Err(e) => {
                        println!("❌ 读取响应时出错: {}", e);
                        break;
                    }
                }
            }
            Err(e) => {
                println!("❌ 读取输入时出错: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}
