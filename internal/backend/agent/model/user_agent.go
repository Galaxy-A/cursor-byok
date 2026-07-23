package modeladapter

const (
	// ClaudeCodeUserAgent 用于将渠道模型请求伪装为 Claude Code 客户端。
	ClaudeCodeUserAgent = "claude-cli/2.1.19 (external, sdk-cli)"
	// AnthropicClaudeCodeUserAgent 用于 Anthropic provider 的 Claude Code UA 兼容。
	AnthropicClaudeCodeUserAgent = "claude-cli/1.0.25"
	// CodexCLIOriginator 是 Codex CLI 官方请求使用的客户端来源标识。
	CodexCLIOriginator = "codex_cli_rs"
	// CodexCLIUserAgent 是 Codex 出站协议使用的客户端版本标识。
	CodexCLIUserAgent = "codex_cli_rs/0.144.6"
)
