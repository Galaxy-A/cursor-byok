package modeladapter

import (
	"encoding/json"
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
)

type codexOutboundIdentity struct {
	InstallationID string
	SessionID      string
	ThreadID       string
	TurnID         string
	WindowID       string
	TurnMetadata   string
}

type codexOutboundTurnMetadata struct {
	InstallationID      string `json:"installation_id"`
	SessionID           string `json:"session_id"`
	ThreadID            string `json:"thread_id"`
	TurnID              string `json:"turn_id"`
	WindowID            string `json:"window_id"`
	RequestKind         string `json:"request_kind"`
	Sandbox             string `json:"sandbox"`
	TurnStartedAtUnixMS int64  `json:"turn_started_at_unix_ms"`
}

func buildCodexOutboundIdentity(req StreamRequest) codexOutboundIdentity {
	if !req.CodexOutboundEnabled {
		return codexOutboundIdentity{}
	}

	threadID := codexOutboundThreadID(req)
	installationID := codexOutboundUUID(strings.Join([]string{
		"cursor-byok-codex-installation",
		strings.TrimSpace(req.ResolvedChannelID),
		strings.TrimSpace(req.BaseURL),
		strings.TrimSpace(req.ProviderModelID),
	}, "\x00"))
	turnID := codexOutboundUUID(firstNonEmptyCodexSeed(req.RequestID, req.ModelCallID, req.RunID, threadID))
	windowID := threadID + ":0"
	metadata := codexOutboundTurnMetadata{
		InstallationID:      installationID,
		SessionID:           threadID,
		ThreadID:            threadID,
		TurnID:              turnID,
		WindowID:            windowID,
		RequestKind:         "turn",
		Sandbox:             "none",
		TurnStartedAtUnixMS: time.Now().UnixMilli(),
	}
	metadataJSON, _ := json.Marshal(metadata)

	return codexOutboundIdentity{
		InstallationID: installationID,
		SessionID:      threadID,
		ThreadID:       threadID,
		TurnID:         turnID,
		WindowID:       windowID,
		TurnMetadata:   string(metadataJSON),
	}
}

func codexOutboundThreadID(req StreamRequest) string {
	seed := firstNonEmptyCodexSeed(req.ConversationID, req.RequestID, req.RunID, req.ModelCallID)
	return codexOutboundUUID("cursor-byok-codex-thread\x00" + seed)
}

func codexOutboundUUID(seed string) string {
	seed = strings.TrimSpace(seed)
	if parsed, err := uuid.Parse(seed); err == nil {
		return parsed.String()
	}
	if seed == "" {
		seed = "cursor-byok-codex"
	}
	return uuid.NewSHA1(uuid.NameSpaceURL, []byte(seed)).String()
}

func firstNonEmptyCodexSeed(values ...string) string {
	for _, value := range values {
		if trimmed := strings.TrimSpace(value); trimmed != "" {
			return trimmed
		}
	}
	return "cursor-byok-codex"
}

func applyOpenAICodexOutboundHeaders(httpReq *http.Request, req StreamRequest, identity codexOutboundIdentity) {
	if httpReq == nil || !req.CodexOutboundEnabled {
		return
	}
	httpReq.Header.Set("User-Agent", CodexCLIUserAgent)
	httpReq.Header.Set("originator", CodexCLIOriginator)
	httpReq.Header.Set("Accept", "text/event-stream")
	httpReq.Header.Set("session-id", identity.SessionID)
	httpReq.Header.Set("thread-id", identity.ThreadID)
	httpReq.Header.Set("x-client-request-id", identity.ThreadID)
	httpReq.Header.Set("x-codex-installation-id", identity.InstallationID)
	httpReq.Header.Set("x-codex-window-id", identity.WindowID)
	httpReq.Header.Set("x-codex-turn-metadata", identity.TurnMetadata)
}

func applyOpenAICodexOutboundBody(body map[string]any, req StreamRequest) {
	if len(body) == 0 || !req.CodexOutboundEnabled {
		return
	}
	body["stream"] = true
	body["store"] = false
	body["prompt_cache_key"] = codexOutboundThreadID(req)

	if len(req.Tools) > 0 {
		body["parallel_tool_calls"] = true
		if _, exists := body["tool_choice"]; !exists {
			body["tool_choice"] = "auto"
		}
	}

	reasoning, ok := body["reasoning"].(map[string]any)
	if !ok || reasoning == nil {
		if strings.TrimSpace(req.ReasoningEffort) == "" {
			return
		}
		reasoning = map[string]any{"effort": strings.TrimSpace(req.ReasoningEffort)}
		body["reasoning"] = reasoning
	}
	if _, exists := reasoning["summary"]; !exists {
		reasoning["summary"] = "auto"
	}
	body["include"] = appendUniqueCodexInclude(body["include"], "reasoning.encrypted_content")
}

func appendUniqueCodexInclude(raw any, required string) []string {
	items := make([]string, 0, 2)
	seen := make(map[string]struct{})
	appendItem := func(value string) {
		value = strings.TrimSpace(value)
		if value == "" {
			return
		}
		if _, exists := seen[value]; exists {
			return
		}
		seen[value] = struct{}{}
		items = append(items, value)
	}

	switch values := raw.(type) {
	case []string:
		for _, value := range values {
			appendItem(value)
		}
	case []any:
		for _, value := range values {
			if text, ok := value.(string); ok {
				appendItem(text)
			}
		}
	}
	appendItem(required)
	return items
}
