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
| 🌐 **HTTP API** | RESTful 接口和 WebSocket | ✅ 完成 |

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

## 📁 项目结构 (DDD 架构)

```
config-master/
├── 📂 src/
│   ├── 📂 domain/                    # 🎯 领域层 - 核心业务逻辑
│   │   ├── 📂 entities/              # 实体对象
│   │   │   ├── 🗂️ configuration.rs  # 配置实体
│   │   │   ├── 🗂️ template.rs       # 模板实体
│   │   │   └── 🗂️ validation_rule.rs # 验证规则
│   │   ├── 📂 value_objects/         # 值对象
│   │   │   ├── 🗂️ config_format.rs  # 配置格式
│   │   │   ├── 🗂️ config_path.rs    # 配置路径
│   │   │   └── 🗂️ environment.rs    # 环境标识
│   │   ├── 📂 repositories/          # 仓储接口
│   │   │   └── 🗂️ configuration_repository.rs
│   │   ├── 📂 services/              # 领域服务
│   │   │   ├── 🗂️ format_converter.rs # 格式转换
│   │   │   └── 🗂️ env_override.rs   # 环境变量覆盖
│   │   └── 📂 events/                # 领域事件
│   │       └── 🗂️ config_changed.rs
│   │
│   ├── 📂 application/               # 🚀 应用层 - 用例协调
│   │   ├── 📂 services/              # 应用服务
│   │   │   ├── 🗂️ configuration_service.rs
│   │   │   ├── 🗂️ template_service.rs
│   │   │   └── 🗂️ validation_service.rs
│   │   ├── 📂 handlers/              # 事件处理器
│   │   └── 📂 dtos/                  # 数据传输对象
│   │       └── 🗂️ config_dto.rs
│   │
│   ├── 📂 infrastructure/            # 🔧 基础设施层 - 技术实现
│   │   ├── 📂 repositories/          # 仓储实现
│   │   │   └── 🗂️ file_config_repository.rs
│   │   ├── 📂 serializers/           # 序列化器
│   │   │   ├── 🗂️ yaml_serializer.rs
│   │   │   ├── 🗂️ json_serializer.rs
│   │   │   └── 🗂️ toml_serializer.rs
│   │   ├── 📂 watchers/              # 文件监控
│   │   │   └── 🗂️ hot_reload.rs
│   │   └── 📂 notification/          # 通知实现
│   │       ├── 🗂️ tcp_notifier.rs
│   │       └── 🗂️ websocket_notifier.rs
│   │
│   ├── 📂 interfaces/                # 🌐 接口层 - 对外接口
│   │   ├── 📂 http/                  # HTTP/REST API
│   │   │   ├── 🗂️ routes.rs
│   │   │   └── 📂 controllers/
│   │   │       └── 🗂️ config_controller.rs
│   │   ├── 📂 websocket/             # WebSocket 实时推送
│   │   │   └── 🗂️ handler.rs
│   │   ├── 📂 tcp/                   # TCP 长连接
│   │   │   └── 🗂️ server.rs
│   │   └── 📂 cli/                   # 命令行界面
│   │       ├── 🗂️ commands.rs
│   │       └── 📂 handlers/
│   │
│   ├── 📂 shared/                    # 🔗 共享组件
│   │   ├── 🗂️ errors.rs             # 统一错误定义
│   │   ├── 🗂️ config.rs             # 应用配置
│   │   └── 🗂️ utils.rs              # 工具函数
│   │
│   ├── 🗂️ lib.rs                    # 库入口
│   └── 🗂️ main.rs                   # 主程序入口
│
├── 📂 examples/                      # 示例代码
│   ├── 🗂️ tcp_client.rs
│   ├── 🗂️ http_client.rs
│   └── 🗂️ websocket_client.rs
│
├── 📂 tests/                         # 测试套件
│   ├── 📂 unit/                      # 单元测试
│   ├── 📂 integration/               # 集成测试
│   └── 📂 e2e/                       # 端到端测试
│
├── 📄 Cargo.toml
└── 📄 README.md
```

## 🏗️ DDD 架构设计

ConfigMaster 采用 **领域驱动设计 (Domain-Driven Design)** 架构，确保代码的清晰性、可维护性和可扩展性。

### 🎯 四层架构

| 层次 | 职责 | 主要组件 |
|------|------|---------|
| **🌐 Interface Layer** | 对外接口，处理用户交互 | HTTP API、CLI、WebSocket、TCP |
| **🚀 Application Layer** | 应用逻辑，协调各种服务 | 应用服务、事件处理器、DTO |
| **🎯 Domain Layer** | 核心业务逻辑，独立于技术细节 | 实体、值对象、领域服务、仓储接口 |
| **🔧 Infrastructure Layer** | 技术实现，具体的技术细节 | 文件系统、序列化器、通知机制 |

### 🔄 依赖关系

```
Interface Layer    ──→  Application Layer
       ↓                     ↓
Infrastructure Layer ←── Domain Layer
```

**关键原则：**
- 📈 **依赖倒置**：上层依赖下层，但通过接口隔离
- 🎯 **业务优先**：Domain Layer 是整个系统的核心
- 🔧 **技术无关**：业务逻辑不依赖具体技术实现
- 🧪 **易于测试**：每一层都可以独立测试

### ✨ DDD 架构优势

1. **🎯 清晰的业务表达**
   - 代码直接反映业务概念
   - 技术人员和业务专家可以用同一套语言交流

2. **🧪 高度可测试性**
   - Domain Layer 无外部依赖，纯业务逻辑
   - 通过接口隔离，便于 Mock 测试

3. **🚀 优秀的扩展性**
   - 新增存储方式只需实现新的 Repository
   - 新增接口类型只需添加新的 Interface Layer

4. **🔧 降低耦合度**
   - 层与层之间通过接口通信
   - 修改某一层不影响其他层

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

### ✅ 已完成 (v0.1.0)
- [x] 🌐 HTTP/RESTful API
- [x] 🔌 WebSocket 实时推送  
- [x] 🔧 DDD 架构重构规划

### 🎯 下一版本 (v0.2.0)
- [ ] 🔐 JWT 认证机制
- [ ] 📊 Prometheus 指标
- [ ] 🎨 Web 管理界面
- [ ] 📝 OpenAPI 文档生成

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