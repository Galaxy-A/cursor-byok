import type {
  JsonValue,
  NetworkEventStream,
  NetworkResponse,
  PluginContext,
} from "cursor-byok:plugin";
import type { LlmRequest, ModelEvent } from "cursor-byok:provider";
import type { ResourceSnapshot } from "cursor-byok:resource";
import { antigravityProvider, isQuotaError } from "./provider.ts";
import { RESOURCE_TYPE } from "./resources.ts";
import {
  ANTIGRAVITY_DAILY_ENDPOINT,
  ANTIGRAVITY_USER_AGENT,
  STATIC_ANTIGRAVITY_MODELS,
} from "./models.ts";

function assert(condition: unknown, message = "assertion failed"): asserts condition {
  if (!condition) throw new Error(message);
}

function assertEquals(actual: unknown, expected: unknown): void {
  const left = JSON.stringify(actual);
  const right = JSON.stringify(expected);
  if (left !== right) throw new Error(`expected ${right}, received ${left}`);
}

type RequestInit = { body?: string; headers?: Record<string, string> };
type FetchHandler = (url: string, init?: RequestInit) => NetworkResponse;
type StreamHandler = (url: string, init?: RequestInit) => NetworkEventStream;

function context(handlers: { fetch?: FetchHandler; stream?: StreamHandler }): PluginContext {
  return {
    network: {
      fetch: (url, init) => {
        if (!handlers.fetch) throw new Error("fetch was not expected");
        return Promise.resolve(handlers.fetch(url, init));
      },
      stream: (url, init) => {
        if (!handlers.stream) throw new Error("stream was not expected");
        return Promise.resolve(handlers.stream(url, init));
      },
    },
    signal: new AbortController().signal,
  };
}

function snapshot(privateData: JsonValue): ResourceSnapshot {
  return {
    id: "resource-1",
    type: RESOURCE_TYPE,
    key: "antigravity:user-1",
    privateData,
    state: { status: "ready" },
  };
}

async function* sse(lines: string[]): AsyncGenerator<string> {
  for (const line of lines) yield line;
}

function request(): LlmRequest {
  return {
    instructions: "You are an AI coding assistant.",
    messages: [{ role: "user", content: [{ type: "text", text: "Hello" }] }],
    tools: [],
    reasoning: { enabled: true, effort: "medium" },
    latency: "standard",
    maxOutputTokens: 65536,
    cacheKey: "conv-1",
  };
}

Deno.test("provider parses usage metadata including cachedContentTokenCount as cacheReadTokens", async () => {
  let requestBody = "";
  let requestHeaders: Record<string, string> = {};
  const events: ModelEvent[] = [];

  const result = await antigravityProvider.invoke(
    {
      model: {
        id: "gemini-3.7-flash-medium",
        displayName: "Gemini 3.7 Flash Medium",
        privateData: {},
      },
      resource: snapshot({
        accessToken: "mock-access-token",
        refreshToken: "mock-refresh-token",
        expiresAt: Date.now() + 3600_000,
        projectId: "test-project-123",
      }),
      request: request(),
    },
    { emit: (event) => events.push(event) },
    context({
      stream: (url, init) => {
        assert(url.includes("/v1internal:streamGenerateContent?alt=sse"));
        requestBody = init?.body ?? "";
        requestHeaders = init?.headers ?? {};
        return {
          status: 200,
          headers: {},
          lines: sse([
            'data: {"response":{"candidates":[{"content":{"parts":[{"thought":true,"text":"Thinking..."}]}}]}}',
            'data: {"response":{"candidates":[{"content":{"parts":[{"text":"Hello world"}]}}]}}',
            'data: {"response":{"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":120,"candidatesTokenCount":15,"totalTokenCount":135,"cachedContentTokenCount":80}}}',
          ]),
        };
      },
    }),
  );

  assertEquals(result, { status: "completed" });
  assertEquals(requestHeaders["authorization"], "Bearer mock-access-token");
  assertEquals(requestHeaders.accept, "text/event-stream");
  assert(!("session-id" in requestHeaders), "Codex-only session-id header was sent");
  assert(!("thread-id" in requestHeaders), "Codex-only thread-id header was sent");
  assert(!("x-client-request-id" in requestHeaders), "request-id header was reused as cache key");
  const body = JSON.parse(requestBody) as Record<string, unknown>;
  assertEquals(body.project, "test-project-123");
  assertEquals(body.model, "gemini-3.7-flash-medium");
  assert(!("sessionId" in body), "unsupported sessionId field was sent");
  assert(!("conversationId" in body), "unsupported conversationId field was sent");

  assertEquals(events, [
    { type: "thinking-start" },
    { type: "thinking-delta", text: "Thinking..." },
    { type: "thinking-end" },
    { type: "text-start" },
    { type: "text-delta", text: "Hello world" },
    { type: "text-end" },
    {
      type: "usage",
      usage: {
        inputTokens: 120,
        outputTokens: 15,
        totalTokens: 135,
        cacheReadTokens: 80,
        cacheWriteTokens: null,
        reasoningTokens: null,
      },
    },
    { type: "done", reason: "stop" },
  ]);
});

Deno.test("isQuotaError identifies rate limits and quota exhaustion", () => {
  assert(isQuotaError("RESOURCE_EXHAUSTED: quota exceeded"));
  assert(isQuotaError("Rate limit exceeded for model"));
  assert(isQuotaError("HTTP 429 Too Many Requests"));
  assert(!isQuotaError("Invalid authorization header"));
});

Deno.test("Gemini 3.8 variants preserve model IDs and use the updated client headers", async () => {
  const ids = ["high", "medium", "low", "tiered"].map((tier) => `gemini-3.8-flash-${tier}`);
  for (const id of ids) {
    assert(STATIC_ANTIGRAVITY_MODELS.some((model) => model.id === id));
  }
  for (const id of ["gemini-3.8-flash", ...ids]) {
    let dispatched = false;
    const result = await antigravityProvider.invoke(
      {
        model: { id, displayName: id, privateData: {} },
        resource: snapshot({
          accessToken: "mock-access-token",
          refreshToken: "mock-refresh-token",
          expiresAt: Date.now() + 3600_000,
          projectId: "test-project",
        }),
        request: request(),
      },
      { emit: () => {} },
      context({
        stream: (url, init) => {
          dispatched = true;
          assert(url.startsWith(`${ANTIGRAVITY_DAILY_ENDPOINT}/`));
          assertEquals(JSON.parse(init?.body ?? "{}").model, id);
          assertEquals(init?.headers?.["user-agent"], ANTIGRAVITY_USER_AGENT);
          assert(!("x-client-version" in (init?.headers ?? {})));
          assert(!("x-client-name" in (init?.headers ?? {})));
          return {
            status: 200,
            headers: {},
            lines: sse(['data: {"response":{"candidates":[{"finishReason":"STOP"}]}}']),
          };
        },
      }),
    );
    assert(dispatched);
    assertEquals(result, { status: "completed" });
  }
});
