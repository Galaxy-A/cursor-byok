<img  width="820"  alt="image" src="https://github.com/user-attachments/assets/2e1710b0-cdbd-4576-bd24-1614df016219" />

<img width="820"  alt="image" src="https://github.com/user-attachments/assets/00885453-6a91-4052-aadf-f686daeec881" />

<img  width="820"  alt="image" src="https://github.com/user-attachments/assets/a607be84-a738-4e33-9750-13352e74001c" />

## 项目来源

本仓库基于 [leookun/cursor-byok](https://github.com/leookun/cursor-byok) fork 并进行自用修改。原项目的作者署名、版权声明和许可证保持不变；本仓库新增功能及构建发布配置见提交记录。



## 为什么做这个项目

公司喜欢把 Agent 服务与模型绑定在一起，让用户只能在指定模型、指定订阅和指定计费方式下使用工具。

我希望打破这种绑定关系：模型应该可以自由选择。开发者应该能够把自己的模型 API 接入到任何 IDE、Chat、Agent 或开发工具中，也可以自托管整套服务，避免被单一平台锁定。

这个项目的目标，是让模型选择权重新回到用户手里。

## 路线图

[正式版路线图](https://github.com/leookun/cursor-byok/discussions/32)
[详细使用教程](https://dcne38qm5vlg.feishu.cn/wiki/JeP7wdGnziBXuikNaF5czWbrn8c)

## Codex 出站协议

对于只接受 Codex 官方客户端请求的 OpenAI 中转站，可以在对应的 OpenAI 模型配置中开启“启用 Codex 出站协议”。开启后，请求链路为：

```text
Cursor 请求
-> Cursor助手
-> Codex CLI Responses 请求
-> OpenAI 官网或中转站
-> Cursor助手转换 Responses SSE
-> Cursor AgentServerMessage
```

### 配置方式

1. 打开 Cursor助手的模型配置。
2. 新建或编辑一个类型为 OpenAI 的模型。
3. 开启“启用 Codex 出站协议”。
4. 使用 `/v1/responses` 端点；选择自定义路径时，API 地址必须以 `/responses` 结尾。

对应的 `config.yaml` 配置示例：

```yaml
modelAdapters:
  - displayName: codex-model
    type: openai
    baseURL: https://api.example.com
    apiKey: YOUR_API_KEY
    modelID: gpt-5-codex
    reasoningEffort: medium
    openAIEndpoint: /v1/responses
    codexOutboundEnabled: true
```

开启后，Cursor助手会补充 Codex CLI 客户端标识和稳定的会话标识，并设置 Codex Responses 所需的请求字段，包括流式输出、prompt cache key、reasoning summary 和工具调用选项。中转站返回的 Responses SSE 会继续转换为 Cursor 可识别的文本、思考、工具调用、使用量和回合结束消息。

该开关仅适用于 OpenAI 类型的模型和 Responses API。关闭开关后，请求恢复为通用 OpenAI 出站协议。自定义请求头在 Codex 默认请求头之后应用，因此可以按中转站要求覆盖默认值。

### 调试验证

在 `~/.cursor-local-assistant-v2/config.yaml` 中开启日志：

```yaml
log: true
```

下一次请求会在 `history/<conversationId>/debug/` 下生成：

- `provider.jsonl`：最终 provider 请求体、响应分片和调用结果。
- `runsse.jsonl`：转换后发送给 Cursor 的消息。
- `bidi.raw.jsonl` 和 `bidi.decoded.jsonl`：Cursor 上行请求的原始与解码记录。
- `runtime.jsonl`：请求状态和 provider pass 流转。

如果关闭开关时中转站返回 `This account only allows Codex official clients`，而开启后能够正常流式响应，说明该中转站的 Codex 客户端限制和本功能均已生效。

## 多平台构建与发布

仓库通过 [`.github/workflows/release.yml`](.github/workflows/release.yml) 分平台构建以下发布资产：

- Windows amd64：`windows-latest`
- macOS amd64：`macos-15-intel`
- macOS arm64：`macos-15`
- Linux amd64：`ubuntu-24.04`
- Linux arm64：`ubuntu-24.04-arm`

在 GitHub 仓库的 Actions 页面手动运行 `Build and release` 时，只构建并保存 Actions Artifact。推送与 `build/config.yml` 中版本一致的标签时，会在所有平台构建成功后生成 `update.json` 并创建或更新 GitHub Release：

```bash
git tag v0.0.40
git push origin v0.0.40
```

工作流需要仓库启用 GitHub Actions，并允许工作流使用 `contents: write` 权限。Linux arm64 使用 GitHub 托管 ARM64 runner；如果当前仓库不可使用 `ubuntu-24.04-arm`，需要把该 runner 标签替换为自托管 ARM64 Linux runner。

当前 macOS 产物使用 ad-hoc 签名，Windows ZIP 未进行 Authenticode 签名。正式分发时，应将签名证书保存到 GitHub Secrets，并在对应构建任务中增加签名和 macOS 公证步骤。

本地 Linux 构建也支持双架构。在对应架构的 Linux 主机上会自动使用原生 CGO 构建；从 macOS/Linux 交叉构建前，需要先创建对应 Docker 镜像：

```bash
wails3 task setup:docker ARCH=amd64
wails3 task setup:docker ARCH=arm64
wails3 task build:linux:amd64
wails3 task build:linux:arm64
```

## 后续

后续会继续扩展更多工具和使用场景，包括但不限于：

- 支持更多 IDE 接入
- 支持更多 Chat 类应用
- 支持更多 Agent 工具和工作流
- 提供更完善的自托管部署方式
- 持续优化不同模型 API 的兼容性
- 降低接入成本，让已有模型额度可以被更充分地利用

最终希望做到：让你的模型 API 可以自由接入到你想使用的任何工具中。




## Star History

<a href="https://www.star-history.com/?repos=leookun%2Fcursor-byok&type=timeline&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=leookun/cursor-byok&type=timeline&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=leookun/cursor-byok&type=timeline&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=leookun/cursor-byok&type=timeline&legend=top-left" />
 </picture>
</a>
