- 支持 GPT 5.6
- OpenAI 模型配置新增 Codex 出站协议开关，支持将 Cursor 请求包装为 Codex CLI Responses 请求，并将响应转换回 Cursor 协议
- Codex 出站模式支持客户端身份头、稳定会话标识、prompt cache key、reasoning summary 和工具调用请求字段
- 增加 Codex 出站模式的 Responses 端点校验与 debug 日志排查说明
- 修复一些bug
- 修复内存泄漏问题，该可能导致内存异常占用
- 支持俄语增加翻译范围
- 修复qwen-3.8-max中断问题(mimo也应该属于同一类问题)
- 修复claude模型可能无法识别图片问题 @GGHansome
- 支持自定义Openai端点 @Sxuan-Coder
- WebSearch 接入百度搜索，DuckDuckGo 作为兜底 @杨超
- 修复一些兼容性问题 @kael-odin 
- 修复window上一些表现问题 @philau2512

🔔 如何让AI自动拉模型配置? (以下为提示词，把地址和密钥换为你的)
我的模型配置在 ～/.cursor-local-assistant-v2/config.yaml，我的API地址是：https://xxx 密钥是xxx，帮我拉所有模型配置进去，不要影响已有模型，根据models标准接口拉取。
