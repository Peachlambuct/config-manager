use std::io::Write;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:8080").await?;
    let mut reader = BufReader::new(stream);

    println!("🚀 已连接到配置管理服务器 127.0.0.1:8080");
    loop {
        let mut input = String::new();
        print!("config-cli> ");
        std::io::stdout().flush()?;

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

                if command.starts_with("listen") {
                    let path = command.split_whitespace().nth(1).unwrap();
                    println!("🔄 开始监听配置文件: {}", path);
                    // 监听配置文件, loop 读取配置文件
                    println!("🔄 开始监听配置文件变化...");
                    loop {
                        println!("⏳ 等待服务器推送...");
                        let response = String::new();
                        if let Err(e) = reader_read_byte(&mut reader, response).await {
                            println!("<UNK> <UNK>: {}", e);
                            break;
                        }
                    }
                    continue;
                }

                // 读取服务器响应
                let response = String::new();
                if let Err(e) = reader_read_byte(&mut reader, response).await {
                    println!("<UNK> <UNK>: {}", e);
                    break;
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

async fn reader_read_byte(reader: &mut BufReader<TcpStream>, response: String) -> std::io::Result<usize> {
    let mut response = response;
    match reader.read_line(&mut response).await {
        Ok(0) => {
            println!("🔌 服务器关闭了连接");
            Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, ""))
        }
        Ok(_) => {
            let response = response.trim();
            let response_bytes_len = response.parse::<usize>().unwrap();
            let mut buffer = vec![0; response_bytes_len];
            reader.read_exact(&mut buffer).await?;
            let response = String::from_utf8(buffer).unwrap();
            if response.starts_with("无效的命令") {
                println!("⚠️  {}", response);
                println!("💡 输入 'help' 查看可用命令");
            } else {
                println!("✅ {}", response);
            }
            Ok(response_bytes_len)
        }
        Err(e) => {
            println!("❌ 读取响应时出错: {}", e);
            Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, ""))
        }
    }
}