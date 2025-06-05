# 🔧 ConfigMaster

**一个强大的统一配置管理工具，支持多格式、实时推送、环境变量覆盖的现代化配置管理解决方案**

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](README.md)

## 🎯 项目简介

ConfigMaster 是一个用 Rust 开发的高性能配置管理工具，专为微服务架构和云原生应用设计。它提供了统一的配置管理接口，支持多种配置格式，并具备实时推送能力。

### ✨ 核心特性

| 功能模块 | 描述 | 状态 |
|---------|------|------|
| 🔄 **多格式支持** | YAML、JSON、TOML 格式互转 | ✅ 完成 |
| 🌍 **环境变量覆盖** | 智能环境变量注入和覆盖机制 | ✅ 完成 |
| 📋 **配置验证** | 类型检查、必填字段、自定义规则 | ✅ 完成 |
| 🔥 **热重载** | 文件变化实时检测和推送 | ✅ 完成 |
| 🖥️ **CLI 工具** | 完整的命令行操作界面 | ✅ 完成 |
| 🌐 **TCP 服务** | 持久连接和实时配置推送 | ✅ 完成 |
| 📊 **日志系统** | 结构化日志记录和审计 | ✅ 完成 |
| 🌐 **HTTP API** | RESTful 接口和 WebSocket | 🚧 开发中 |

## 🛠️ 技术栈

### 🦀 核心技术
- **Rust** - 系统编程语言，保证性能和安全性
- **Tokio** - 异步运行时，支持高并发处理
- **Serde** - 序列化/反序列化框架

### 📦 主要依赖
```toml
[dependencies]
tokio = { version = "1.45", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
toml = "0.8"
clap = { version = "4.0", features = ["derive"] }
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
notify = "8.0"
chrono = { version = "0.4", features = ["serde"] }
colored = "2.0"
```

## 🚀 快速开始

### 📥 安装

```bash
# 克隆项目
git clone https://github.com/yourusername/config-master.git
cd config-master

# 构建项目
cargo build --release

# 安装到系统
cargo install --path .
```

### 💻 CLI 使用

#### 🔍 验证配置文件
```bash
# 基础验证
config-master validate config.yaml

# 使用验证规则文件
config-master validate config.yaml --validate-file validation.yaml
```

#### 📄 查看配置内容
```bash
# 美化显示配置
config-master show config.yaml

# 获取特定配置项
config-master show config.yaml --get database.host

# 控制显示深度
config-master show config.yaml --depth 3
```

#### 🔄 格式转换
```bash
# JSON 转 YAML
config-master convert config.json config.yaml

# TOML 转 JSON
config-master convert config.toml config.json
```

#### 📋 生成配置模板
```bash
# 生成数据库配置模板
config-master template database --format yaml

# 生成 Redis 配置模板
config-master template redis --format json

# 支持的模板: database, redis, webserver, logger, monitor
```

### 🌐 服务模式

#### 启动配置服务器
```bash
# 默认配置启动
config-master serve

# 自定义端口和路径
config-master serve --port 9090 --host 0.0.0.0 --config-path ./configs
```

#### 📱 客户端连接示例
```bash
# 使用内置 TCP 客户端
cargo run --example tcp_send

# 客户端命令示例
config-cli> add app.yaml      # 添加配置文件
config-cli> get app.yaml      # 获取配置内容
config-cli> list              # 列出所有配置
config-cli> listen app.yaml   # 监听配置变化
config-cli> remove app.yaml   # 删除配置文件
```

## 📁 项目结构

```
config-master/
├── 📂 src/
│   ├── 📂 model/
│   │   ├── 🗂️ config.rs      # 配置数据结构和解析
│   │   ├── 🗂️ template.rs    # 配置模板生成
│   │   ├── 🗂️ validation.rs  # 配置验证框架
│   │   ├── 🗂️ app.rs         # 应用状态管理
│   │   └── 🗂️ log.rs         # 日志系统
│   ├── 🗂️ command.rs         # 命令行定义
│   ├── 🗂️ handler.rs         # 业务逻辑处理
│   ├── 🗂️ error.rs           # 错误类型定义
│   ├── 🗂️ lib.rs             # 库入口
│   └── 🗂️ main.rs            # 主程序入口
├── 📂 example/
│   └── 🗂️ tcp_send.rs        # TCP 客户端示例
├── 📄 Cargo.toml
└── 📄 README.md
```

## 🌟 主要功能详解

### 🔄 多格式配置支持
ConfigMaster 支持主流配置格式的无损转换：

```yaml
# config.yaml
database:
  host: localhost
  port: 3306
  credentials:
    username: admin
    password: secret
```

```json
// config.json (转换结果)
{
  "database": {
    "host": "localhost", 
    "port": 3306,
    "credentials": {
      "username": "admin",
      "password": "secret"
    }
  }
}
```

### 🌍 环境变量覆盖
支持智能的环境变量覆盖机制：

```bash
# 设置环境变量
export APP_DATABASE_HOST=prod-db.example.com
export APP_DATABASE_PORT=5432

# 原配置会被环境变量覆盖
# database.host: localhost -> prod-db.example.com
# database.port: 3306 -> 5432
```

### 📋 配置验证
强大的配置验证框架：

```yaml
# validation.yaml
required_fields:
  - "database.host"
  - "database.credentials.username"

field_types:
  database.port:
    type: "number"
    min: 1
    max: 65535
  database.host:
    type: "string"
    max_length: 100
```

### 🔥 实时热重载
文件变化自动检测和推送：

```
📁 configs/
├── app.yaml     # 修改此文件
└── ...

🔄 自动检测变化 → 📤 推送给所有监听客户端
```

## 🧪 使用示例

### 场景1：微服务配置管理
```bash
# 启动配置服务
config-master serve --port 8080 --config-path ./microservices-configs

# 各微服务连接并监听自己的配置
user-service: listen user-service.yaml
order-service: listen order-service.yaml
payment-service: listen payment-service.yaml
```

### 场景2：环境切换
```bash
# 开发环境
export APP_ENV=development
export APP_DATABASE_HOST=localhost

# 生产环境  
export APP_ENV=production
export APP_DATABASE_HOST=prod-cluster.example.com

# 相同配置文件，不同环境变量，自动适配
```

### 场景3：配置验证流水线
```bash
# 在 CI/CD 中验证配置
config-master validate prod-config.yaml --validate-file schema.yaml

# 验证通过后部署
if [ $? -eq 0 ]; then
    echo "✅ 配置验证通过，开始部署"
    deploy_application
fi
```

## 📊 性能特点

- 🚀 **高性能**: Rust 零开销抽象，内存安全
- ⚡ **低延迟**: 实时配置推送，毫秒级响应
- 🔒 **类型安全**: 编译时类型检查，运行时验证
- 🛡️ **内存安全**: 无数据竞争，无内存泄漏
- 📈 **高并发**: 基于 Tokio 异步运行时

## 🗺️ 发展规划

### 🎯 下一版本 (v0.2.0)
- [ ] 🌐 HTTP/RESTful API
- [ ] 🔌 WebSocket 实时推送  
- [ ] 🔐 JWT 认证机制
- [ ] 📊 Prometheus 指标
- [ ] 🎨 Web 管理界面

### 🚀 未来规划 (v0.3.0+)
- [ ] 🏘️ 分布式部署支持
- [ ] 🔄 Raft 共识算法
- [ ] 💾 持久化存储引擎
- [ ] 🌍 多数据中心同步
- [ ] 📈 配置变更历史追踪

## 🤝 贡献指南

欢迎贡献代码！请遵循以下步骤：

1. 🍴 Fork 项目
2. 🌿 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 💾 提交更改 (`git commit -m 'Add amazing feature'`)
4. 📤 推送分支 (`git push origin feature/amazing-feature`)
5. 🔀 创建 Pull Request

### 🧪 运行测试
```bash
# 单元测试
cargo test

# 集成测试
cargo test --test integration

# 性能基准测试
cargo bench
```

## 👥 作者

- **Peachlambuct** - [GitHub](https://github.com/Peachlambuct)

## 🙏 致谢

- Rust 社区提供的优秀 crate
- 所有贡献者的宝贵建议
- 早期用户的反馈和支持

---

<div align="center">

**⭐ 如果这个项目对你有帮助，请给个 Star！⭐**

[🏠 主页](https://github.com/Peachlambuct/config-master) · 
[🐛 报告问题](https://github.com/Peachlambuct/config-master/issues) · 
[💡 功能请求](https://github.com/Peachlambuct/config-master/issues)

</div> 