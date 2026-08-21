# Cursor助手项目：多智能体协同开发、动态测试与浏览器自动化工程规范

> 本文是针对本仓库的项目适配版。它继承“先分析、再设计、动态测试、失败后回归”的原则，但不套用传统企业后台项目的数据库、RBAC、Controller/Mapper 或固定测试清单假设。

## 1. 项目边界与真实架构

本项目是一个 **Go + Wails 3 + Vue/Vite** 桌面应用，主要职责是为 Cursor 提供模型适配、协议桥接、本地运行时、代理和调试能力。仓库同时包含独立的 Cursor 协议 HTTPS MITM 调试器。

### 1.1 主要层次

| 层次 | 主要目录 | 关注点 |
| --- | --- | --- |
| 桌面入口与运行时 | `main.go`、`internal/app/`、`internal/runtime/` | Wails 生命周期、窗口、托盘、启动与退出 |
| 前端界面 | `frontend/src/` | Vue 页面、状态、模型配置、国际化、Wails 调用 |
| Go 后端与桥接 | `internal/backend/`、`internal/bridge/` | 本地 HTTP 服务、forwarder、agent、provider、工具交互 |
| 协议与生成代码 | `proto/`、`gen/`、`frontend/bindings/` | protobuf/Connect 协议、Wails 前端绑定 |
| Cursor 客户端适配 | `internal/client/`、`internal/cursor/`、`internal/mitm/` | Cursor 账户、证书、请求观察、网络代理 |
| 独立调试器 | `cursor-proxy-debugger/`、`cmd/cursor-proxy-debugger/` | `BidiAppend`、`RunSSE`、帧解码、抓包 UI |
| 本地持久化与诊断 | `internal/appdata/`、`internal/backend/forwarder/` | 配置、history、state/context、debug 日志 |
| 构建与发布 | `Taskfile.yml`、`build/`、`.github/workflows/` | 前端构建、绑定生成、跨平台打包、发布资产 |

### 1.2 不应默认套用的模型

- **没有业务数据库迁移链路**：默认不设计 SQL、表、Mapper、外键或数据库回滚。若未来引入数据库，必须单独补充架构说明、迁移文件和数据备份策略。
- **没有传统 RBAC/菜单权限模型**：默认不要求普通用户/管理员双账号矩阵。需要验证的是本地配置边界、敏感操作保护、API 本地访问范围和 Cursor 客户端信任关系。
- **不是纯 Web 应用**：Wails 原生窗口、系统代理、证书和 Cursor 客户端行为不能仅靠浏览器页面判断。
- **不新增测试代码**：仓库技能 `test-requirements` 明确规定代码禁止写任何测试。可以运行已有测试、构建检查和手工/浏览器自动化，但不要为了本次需求新增 `_test.go`、Vitest、Playwright 用例文件或测试脚本。

## 2. 总体研发原则

每项开发任务遵循：

```text
需求分析 → 项目分析 → 方案设计 → 实现 → 独立审查 → 影响分析
→ 动态测试设计 → 环境启动 → 真实链路验证 → 修复 → 动态回归 → 验收
```

### 2.1 先读取项目事实，再修改

开始编码前必须确认：

1. 用户要改变的行为、可观察结果和不在范围内的内容。
2. 入口属于 Vue 页面、Wails binding、Go service、forwarder、provider、协议生成代码、Cursor 适配层还是独立调试器。
3. 请求/响应数据流：Cursor → 本地服务 → forwarder/provider → 上游，以及返回方向的 SSE/AgentServerMessage 转换。
4. 是否涉及 `history/<conversationId>/state.json`、`context.json`、`debug/`、配置文件、证书或系统代理。
5. 是否需要重新生成 proto、Connect/Wails bindings、前端 i18n catalog 或第三方声明。
6. 本次修改会影响哪些平台：Windows、macOS、Linux，是否涉及平台专属证书或网络代码。
7. 现有测试、构建和启动入口是否可用；不要凭空假设存在后端端口、数据库、登录页或测试账号。

### 2.2 项目内技能是设计约束的一部分

涉及下列主题时，先读取对应 `.agents/skills/` 文档并按其要求执行：

- 协议、agent、本地模式、provider 或 history-store：`coding-guidance`、`cursor-client-e2e-debugging`。
- prompt、历史回放、动态 reminder、模型请求构造：`prefix-cache-stability`。
- 前端文案、locale、catalog、托盘标签：`i18n-requirements`。
- 测试边界：`test-requirements`。
- 已安装 Cursor 客户端：只读使用 `cursor-app-formatted` 或 `cursor-client-e2e-debugging`，禁止修改 bundle、签名或安装副本。

## 3. 协同角色与交付物

任务复杂度足够时，可拆分为以下角色。小任务可以由一个 Agent 按同样的交付物顺序完成，不要求机械创建多个 Agent。

### 3.1 架构/分析 Agent

交付：需求边界、文件地图、数据流、影响范围、实施方案、测试范围和风险清单。禁止在方案未确认前进行大规模修改。

### 3.2 开发 Agent

交付：最小范围代码变更、必要的生成资产更新、变更说明和本地验证结果。优先复用现有 helper、Wails binding、协议类型和状态管理，不做无关重构。

### 3.3 审查 Agent

独立检查：

- 前端状态、事件、异步 loading、错误提示、空数据、国际化和 Wails 调用。
- Go 并发、生命周期、context 取消、流式响应、重试、资源关闭和跨平台分支。
- 协议字段、帧边界、`request_id`、`model_call_id`、`exec_id`、pass 隔离和迟到消息处理。
- history-store 的 append-only 语义、`state.json` 与 `context.json` 的职责边界、prefix cache 稳定性。
- 敏感信息、证书私钥、API key、Cookie、Authorization、日志脱敏和本地端口暴露。

### 3.4 验证 Agent

根据真实变更动态生成验证矩阵，启动所需环境，执行已有测试/构建、浏览器自动化或真实 Cursor 链路验证，记录 `PASS`、`FAIL`、`BLOCKED`、`N/A`，并在修复后完成回归。

## 4. 标准工作流

### 阶段 A：需求与项目分析

输出一份短设计记录，至少包含：

- 用户场景与验收标准。
- 受影响的入口文件、模块、协议或页面。
- 正向数据流和错误/取消/重试路径。
- 配置、文件、证书、history、日志和生成代码影响。
- 平台差异与兼容性风险。
- 明确的非目标，防止把无关模块纳入回归。

### 阶段 B：方案设计

方案必须说明：

1. 修改哪些文件，为什么不修改其他文件。
2. 是否需要新增抽象；若需要，说明它消除的重复或复杂度。
3. 是否保持历史 prompt、协议顺序、绑定接口和配置兼容。
4. 需要执行哪些生成任务、构建任务和运行时验证。
5. 失败时的回滚/降级方式，尤其是证书、代理和 provider 请求构造。

### 阶段 C：实现

- 保持现有目录边界和命名习惯。
- 先改源文件，再按项目任务生成 `gen/`、`frontend/bindings/` 或 i18n 产物。
- 修改前端用户可见中文时，遵守 i18n scanner 规则，运行 `frontend` 的构建扫描，不手写 generated message id。
- 修改 prompt/history/request 时，确保历史上下文 append-only；易变状态放入最新请求尾部或 `state.json`，不要污染长期 replay 前缀。
- 只读核对已安装 Cursor 客户端，不对其文件、签名或证书做 patch。

### 阶段 D：独立代码审查

审查必须先列问题，再给摘要。重点追问：

- 是否误把通知型 `AgentServerMessage` 当成必须 ack 的请求型消息？
- pending exec 是否严格按 `exec_id`/`message_id` 匹配，是否存在跨 pass 串线？
- 流式响应、`[DONE]`、`stream_close`、迟到工具结果和重连是否安全？
- 错误是否能到达 UI/日志，资源是否在取消和异常时释放？
- 是否泄露 API key、Cookie、Authorization、CA 私钥或完整 provider body？
- 是否引入了不必要的重启、端口变化、生成文件漂移或跨平台回归？

## 5. 运行环境与启动规范

### 5.1 常用命令

以下命令以仓库根目录为基准；Windows 本地开发优先使用 PowerShell，跨平台 Taskfile 任务按当前环境执行。

| 目的 | 命令 |
| --- | --- |
| Wails 开发模式 | `wails3 task dev` 或 `task dev` |
| 仅启动前端 Vite | `wails3 task common:dev:frontend`，默认端口 `9245` |
| 前端依赖 | `wails3 task common:install:frontend:deps` |
| 前端生产构建 | `wails3 task common:build:frontend` |
| 生成 proto/bindings | `wails3 task common:generate:proto`、`wails3 task common:generate:bindings` |
| 运行 Go 现有测试 | `go test ./...` |
| 启动协议调试器 | `go run ./cmd/cursor-proxy-debugger -open=false` |
| 调试器构建 | `go build -o ./bin/cursor-proxy-debugger ./cmd/cursor-proxy-debugger` |

Wails 开发模式会构建开发资源、启动 Vite，并运行桌面程序；不要把 Vite 页面能够打开误判为 Wails 原生窗口、系统代理或 Cursor 链路已经正常。

### 5.2 环境检查

启动前检查：

- `go version`、`node --version`、`yarn --version`、`wails3 version`、`protoc --version`（涉及生成时）。
- 端口 `9245`、调试器代理 `9090`、调试 UI `9091` 是否被占用；先识别进程，再决定是否更换端口或结束进程。
- 是否存在旧的 dev 进程、旧证书、旧配置或残留临时目录。
- Windows/macOS/Linux 专属依赖是否具备；涉及 CGO、WebKit、签名或证书时记录 `BLOCKED` 原因。

## 6. 动态测试设计：测试变更而不是固定清单

测试范围由本次 diff、调用链和风险决定。每一项都必须标记 `PASS`、`FAIL`、`BLOCKED` 或 `N/A`，不能把未执行项目写成 `PASS`。

### 6.1 按变更类型选择验证

| 变更类型 | 最小验证集合 |
| --- | --- |
| Vue 页面、模型配置、状态或 i18n | 相关页面真实操作、空/错误/重复提交、locale 切换；`yarn build`（含 scanner） |
| Go service、配置、bridge、Wails binding | `go test ./...`（仅运行已有测试）、前端构建、启动桌面应用并操作受影响流程 |
| forwarder、provider、SSE、Connect 或协议字段 | 现有 Go 测试、协议帧/流式/取消/重试验证、debug 日志和 provider replay 证据 |
| prompt、history、context、state 或 compaction | 检查 append-only 历史、重启/重连承接、`state.json` 与 `context.json`、prefix/cache 指标 |
| 证书、MITM、系统代理、网络出口 | 本地 CA 下载/信任、目标 host MITM、非目标 CONNECT 透传、敏感头脱敏、恢复原代理设置 |
| 独立调试器 UI | 启动 `9090/9091`，浏览器打开界面，抓包列表/过滤/详情/清空/语言切换，验证内存数据关闭后消失 |
| proto 或生成资产 | 重新生成 proto、bindings，检查 diff 只包含预期产物，再执行对应构建和运行验证 |
| 发布/构建脚本 | 运行对应平台构建或 CI 等价步骤，检查许可证、第三方声明、版本和产物命名 |

### 6.2 异常与边界

仅选择与变更相关的场景：

- 空配置、缺少 API key、非法 URL、未知模型、重复模型渠道、超长输入。
- provider 4xx/5xx、SSE `event: error`、断流、超时、取消、重试和迟到结果。
- `request_id`、`model_call_id`、`exec_id` 不匹配、重复回包、重连和跨 pass 事件。
- history 文件缺失/损坏、旧格式残留、权限不足、磁盘写入失败。
- 端口占用、证书未信任、目标 host 非 MITM、代理恢复失败。
- 空列表、大量抓包、特殊字符、多语言、窗口关闭/最小化/托盘恢复。

不涉及的数据库、管理员账号、OSS、Redis、MQ 等场景标记 `N/A`，并说明项目没有该边界或本次 diff 未触及。

## 7. 浏览器自动化与真实客户端验证

### 7.1 浏览器自动化适用范围

浏览器自动化只验证真实可浏览器化的界面：

- `frontend` 的 Vite 页面和调试器 `http://127.0.0.1:9091`。
- 页面加载、路由、表单、模型配置、错误提示、空数据、过滤、排序、清空、语言切换和 SSE/UI 刷新。

推荐使用仓库外部的 Playwright/浏览器工具临时执行，不新增测试文件；记录 URL、端口、步骤、截图/控制台错误和结果。

### 7.2 桌面与协议链路不能只测浏览器

以下内容必须通过 Wails 桌面运行或真实 Cursor/调试器链路确认：

- 原生窗口、托盘、最小化/关闭、Browser/Window API。
- 系统代理、CA 安装/信任、Windows/macOS/Linux 平台分支。
- Cursor `BidiAppend` 上行、`RunSSE` 下行、AgentServerMessage、工具调用和 provider 转换。
- backend 重连、pending tool 收口和 history-store 持久化。

如果本机缺少 Cursor、目标 provider、平台依赖或凭据，必须标记 `BLOCKED`，写清缺失条件和已完成的替代验证，不得用“编译成功”代替真实链路验收。

### 7.3 调试证据

协议或 provider 问题优先收集：

- `history/<conversationId>/state.json`：当前状态、loop、latest request prefix、last provider call。
- `history/<conversationId>/context.json`：append-only 语义历史。
- `history/<conversationId>/debug/provider.jsonl`、`runsse.jsonl`、`bidi.raw.jsonl`、`bidi.decoded.jsonl`、`runtime.jsonl`。
- `logs/app.log` 及复现时间、`request_id`、`model_call_id`、`tool_call_id`。

不要依赖旧的 `data.sqlite`、`conversation.json`、`turns/<n>/request.json`、`sse.jsonl`、`summary.json` 等已废弃产物。独立调试器抓包只保存在当前进程内存，关闭后消失。

## 8. 回归等级

### L1：本次功能回归

重新执行所有直接受影响的页面、接口、协议或调试器操作。

### L2：关联链路回归

当修改共享模型适配、配置解析、forwarder、provider、状态或协议映射时，回归关联的配置保存、请求发送、流式响应、错误提示和历史承接。

### L3：核心流程回归

以下变更通常需要扩大到桌面启动、模型配置、一次完整 Cursor 请求、工具调用/取消、重连、日志和退出流程：

- Wails 生命周期、全局状态或公共 binding。
- forwarder/provider/router、SSE/Connect 协议和全局错误处理。
- history-store、prompt replay、compaction、prefix cache。
- 证书、系统代理、网络出口或跨平台构建脚本。

未受影响的 UI 页面、独立调试器功能或发布平台可以标记 `N/A`，但必须给出影响分析依据。

## 9. 失败处理与动态回归

发现问题后按以下闭环执行：

```text
记录复现步骤与证据
→ 定位层次与根因
→ 最小修复
→ 独立审查
→ 重跑失败项
→ 重跑受影响的 L1/L2 项
→ 按风险决定是否执行 L3
```

修复验证必须覆盖原始失败输入、相邻边界和一条正常路径。迟到消息、重连、取消和 provider 错误不能只用一次成功请求证明已修复。

## 10. 最终验收清单

- [ ] 需求边界、非目标和项目入口已确认。
- [ ] 影响模块、协议数据流、配置/history/日志/生成资产已分析。
- [ ] 方案和最小修改范围已记录。
- [ ] 实现完成，无无关重构。
- [ ] 已读取并遵守相关 `.agents/skills/` 约束。
- [ ] 独立代码审查完成，问题已处理或记录。
- [ ] 动态测试矩阵已生成，未执行项标为 `BLOCKED` 或 `N/A`。
- [ ] 已运行与变更匹配的已有测试、构建、生成任务或静态检查。
- [ ] 可浏览器化界面已完成真实浏览器验证（如适用）。
- [ ] 桌面、代理、证书、Cursor 协议或 provider 链路已完成真实验证（如适用）。
- [ ] 失败用例已修复并回归，关联功能完成必要验证。
- [ ] 未新增仓库禁止的测试代码。
- [ ] 敏感信息已脱敏，临时代理/证书/进程状态已恢复。
- [ ] 最终报告明确给出 `PASS`、`FAIL`、`BLOCKED`、`N/A` 数量和验收结论。

## 11. 最终测试报告模板

```markdown
# 测试报告：<任务名称>

## 本次修改
- 代码/页面/协议/配置/生成资产：
- 不在范围内的模块：

## 测试范围
- 直接功能：
- 关联链路：
- 使用的环境、端口和账号/凭据条件：
- 未测试项及原因：

## 测试结果
| 状态 | 数量 | 说明 |
| --- | ---: | --- |
| PASS | 0 | |
| FAIL | 0 | |
| BLOCKED | 0 | 缺少什么环境或依赖 |
| N/A | 0 | 为什么与本次变更无关 |

## 回归范围
- L1：
- L2：
- L3：

## 证据
- 命令与输出摘要：
- 浏览器 URL/步骤/截图或控制台结果：
- history/debug/log 证据：

## 最终结论
明确写出本次修改是否通过验收，以及仍存在的 BLOCKED/风险。
```

## 12. 不可违背的项目原则

> 先读项目事实，再设计；先设计，再实现；先分析 diff，再动态测试。
>
> 测试变更，不执行与项目无关的固定清单。
>
> 浏览器验证页面，真实客户端验证桌面和协议；编译通过不等于功能完成。
>
> history 语义历史保持 append-only；最新易变状态不要污染长期 prompt 前缀。
>
> 只读核对已安装 Cursor，不修改客户端 bundle、签名或证书。
>
> 失败必须修复并回归；无法验证必须诚实标记 `BLOCKED`，无关内容标记 `N/A`。
