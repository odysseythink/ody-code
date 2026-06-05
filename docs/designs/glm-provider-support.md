# GLM (Zhipu AI / Z.AI) Provider Support

**Goal:** Add a first-class `glm` provider to the kosong LLM abstraction layer so users can configure and chat with GLM models (e.g., `glm-4.5`, `glm-4.7`) through the standard ody-code TUI and SDK interfaces.

**Audit level:** Deep [C:USER]

---

## Resolved Decisions

| # | Dimension | Decision | Source |
|---|---|---|---|
| 1 | Scope | Support Z.AI (`api.z.ai`) and bigmodel.cn (`open.bigmodel.cn`) endpoints; default to Z.AI; no hard-coded model catalog — users add models via `/model` or provider catalog. | [C:USER] |
| 2 | Data & State | No new persistent data structures; model capabilities rely on runtime `UNKNOWN_CAPABILITY` (no catalog) or user-configured overrides. | [C:INFERRED] |
| 3 | Integration | Use `openai` npm SDK as the HTTP/transport layer but implement a standalone `GLMChatProvider` with custom request/response translation; support full tool calling, streaming, and reasoning. | [C:USER] |
| 4 | Error & Degradation | Re-use `convertOpenAIError` from `openai-common.ts` because GLM returns OpenAI-compatible error shapes. | [C:USER] |
| 5 | Security | API-key only (`GLM_API_KEY` env var or config); no OAuth flow. | [C:USER] |
| 6 | Observability | Re-use existing kosong telemetry and ody-code logging; no GLM-specific metrics. | [C:INFERRED] |
| 7 | Operations | No feature toggle; provider is always available once code ships. | [C:INFERRED] |

---

## Scope In / Out

**In:**
- New `glm` provider type in kosong's `ProviderConfig` union. [C:USER]
- `GLMChatProvider` class implementing `ChatProvider`. [C:USER]
- API-key auth resolution (`GLM_API_KEY` env var). [C:USER]
- Configurable `baseUrl` (default `https://api.z.ai/api/paas/v4/`). [C:USER]
- Standard chat completions (non-streaming + streaming). [C:USER]
- Tool calling (parallel, streaming delta arguments). [C:USER]
- Reasoning/thinking support via `withThinking()`. [C:USER]
- Error handling via existing `convertOpenAIError`. [C:USER]
- Provider registration in `createProvider` switch. [C:INFERRED]
- Unit tests for `GLMChatProvider` message conversion, capability detection, and parameter mapping. [C:INFERRED]

**Out (deferred):**
- OAuth / platform login for GLM. [C:USER] — API-key only.
- Hard-coded model capability catalog. [C:USER] — users configure per-model capabilities.
- Vision/multimodal input (image_url, audio_url, video_url). [C:DEFERRED] — GLM-4.5v supports vision but we defer until a user explicitly requests it; initial implementation rejects non-text content parts with a clear error.
- `clear_thinking` parameter control. [C:DEFERRED] — always defaults to `true` (exclude reasoning from earlier turns); can be added later via `withGenerationKwargs`.
- Custom SSE parser (we use the OpenAI SDK's built-in streaming). [C:USER]

---

## Architecture

```
App/TUI layer
    |
    v
packages/kosong/src/providers/index.ts  createProvider(config)
    |  case 'glm': return new GLMChatProvider(config)
    v
packages/kosong/src/providers/glm.ts    GLMChatProvider
    |
    +---> message conversion (kosong Message -> OpenAI-compatible GLM message)
    |       +---> GLM-specific: map thinking content to reasoning_content field
    |       +---> GLM-specific: inject thinking.type param when withThinking is active
    |       +---> GLM-specific: filter out empty-string text content parts
    |
    +---> OpenAI SDK client.chat.completions.create(params)
    |       (HTTP/SSE handled by SDK; baseUrl points to GLM endpoint)
    |
    +---> response parsing (OpenAI-compatible chunk -> kosong StreamedMessagePart)
    |       +---> extract reasoning_content from delta.message or delta
    |       +---> extract text content
    |       +---> extract tool_calls with incremental argument buffering
    |       +---> extract usage
    |
    +---> error conversion via convertOpenAIError (from openai-common)
```

**Data changes at each arrow:**
1. `createProvider` receives a `ProviderConfig` with `type: 'glm'` → instantiates `GLMChatProvider` with resolved `apiKey`, `baseUrl`, `model`.
2. `GLMChatProvider.generate()` receives `systemPrompt`, `tools[]`, `history[]` → builds OpenAI-compatible `messages[]` array with GLM-specific field injection.
3. OpenAI SDK sends HTTP POST to GLM endpoint → returns `ChatCompletion` or async iterator of `ChatCompletionChunk`.
4. `GLMStreamedMessage` iterates chunks → yields `StreamedMessagePart` (text, think, function, tool_call).

---

## Components

### 1. GLMChatProvider

Location: `packages/kosong/src/providers/glm.ts`

```ts
export interface GLMOptions {
  apiKey?: string | undefined;
  baseUrl?: string | undefined;
  model: string;
  stream?: boolean | undefined;
  maxTokens?: number | undefined;
  httpClient?: unknown;
  defaultHeaders?: Record<string, string>;
}

export interface GLMGenerationKwargs {
  max_tokens?: number | undefined;
  temperature?: number | undefined;
  top_p?: number | undefined;
  stop?: string | string[] | undefined;
  [key: string]: unknown;
}

export class GLMChatProvider implements ChatProvider {
  readonly name: string = 'glm';

  get modelName(): string;
  get thinkingEffort(): ThinkingEffort | null;
  get modelParameters(): Record<string, unknown>;

  getCapability(model?: string): ModelCapability;

  async generate(
    systemPrompt: string,
    tools: Tool[],
    history: Message[],
    options?: GenerateOptions,
  ): Promise<StreamedMessage>;

  withThinking(effort: ThinkingEffort): GLMChatProvider;
  withGenerationKwargs(kwargs: GLMGenerationKwargs): GLMChatProvider;
  withMaxCompletionTokens(maxCompletionTokens: number): GLMChatProvider;
}
```

**Contract:** `GLMChatProvider` translates kosong's native message/tool/reasoning format into GLM's OpenAI-compatible wire format, handles streaming via the OpenAI SDK, and maps GLM's response fields (including `reasoning_content`) back into kosong `StreamedMessagePart`s.

### 2. GLMStreamedMessage

Location: `packages/kosong/src/providers/glm.ts` (private inner class or module-local)

```ts
class GLMStreamedMessage implements StreamedMessage {
  constructor(
    response: OpenAI.Chat.ChatCompletion | AsyncIterable<OpenAI.Chat.ChatCompletionChunk>,
    isStream: boolean,
  );

  get id(): string | null;
  get usage(): TokenUsage | null;
  get finishReason(): FinishReason | null;
  get rawFinishReason(): string | null;

  async *[Symbol.asyncIterator](): AsyncIterator<StreamedMessagePart>;
}
```

**Contract:** Wraps the OpenAI SDK response and yields kosong parts. Handles:
- `delta.reasoning_content` → `{ type: 'think', think: ... }`
- `delta.content` → `{ type: 'text', text: ... }`
- `delta.tool_calls` → incremental tool call buffering via `convertChatCompletionStreamToolCall` from `chat-completions-stream.ts`
- `chunk.usage` → `TokenUsage` via `extractUsage` from `openai-common.ts`
- `choice.finish_reason` → `FinishReason` via `normalizeOpenAIFinishReason`

### 3. Message Conversion Helpers

Location: `packages/kosong/src/providers/glm.ts` (module-local)

```ts
interface GLMMessage {
  role: string;
  content?: string | OpenAIContentPart[] | undefined;
  tool_calls?: OpenAIToolCallOut[] | undefined;
  tool_call_id?: string | undefined;
  name?: string | undefined;
  reasoning_content?: string | undefined; // GLM-specific round-trip field
  [key: string]: unknown;
}

function convertMessage(
  message: Message,
  toolMessageConversion: ToolMessageConversion,
): GLMMessage;
```

**Contract:** Identical to `convertMessage` in `openai-legacy.ts` except:
- Think parts are accumulated into a `reasoning_content` string on the message object (not a separate field).
- Empty-string `text` content parts are filtered out before serialization (workaround for GLM validation error `messages[N].content[M].text: 不能为空`).

---

## Call-Site Integration

### A. Provider registration

**File:** `packages/kosong/src/providers/index.ts`

Insert into imports (line 1-7):
```ts
import { GLMChatProvider, type GLMOptions } from './glm';
```

Modify `ProviderConfig` union (line 9-16):
```ts
export type ProviderConfig =
  | ({ type: 'anthropic' } & AnthropicOptions)
  | ({ type: 'openai' } & OpenAILegacyOptions)
  | ({ type: 'kimi' } & KimiOptions)
  | ({ type: 'google-genai' } & GoogleGenAIOptions)
  | ({ type: 'openai_responses' } & OpenAIResponsesOptions)
  | ({ type: 'vertexai' } & GoogleGenAIOptions)
  | ({ type: 'deepseek' } & DeepSeekOptions)
  | ({ type: 'glm' } & GLMOptions);   // <-- new
```

Modify `createProvider` switch (line 20-40):
```ts
    case 'deepseek':
      return new DeepSeekChatProvider(config);
    case 'glm':
      return new GLMChatProvider(config);
```

### B. Capability detection

**File:** `packages/kosong/src/providers/glm.ts`

GLM models do not have a hard-coded catalog. `getCapability` returns `UNKNOWN_CAPABILITY` for all models unless the caller overrides via config:

```ts
getCapability(model?: string): ModelCapability {
  return UNKNOWN_CAPABILITY;
}
```

This is consistent with the user's decision not to hard-code model lists. Users who need accurate capability badges can set `capabilities` on their `ModelAlias` config.

### C. Request parameter mapping

**File:** `packages/kosong/src/providers/glm.ts` (inside `generate()`)

When `withThinking()` has been called, inject GLM's reasoning control into the request. GLM supports `thinking: { type: 'enabled' | 'disabled' }` [C:UPSTREAM] from Z.AI docs. When `thinkingEffort` is `'off'`, we emit `thinking: { type: 'disabled' }`; otherwise we omit the field (GLM defaults to enabled).

```ts
// Pseudocode inside generate()
const createParams: Record<string, unknown> = {
  model: this._model,
  messages,
  stream: this._stream,
  ...this._generationKwargs,
};

if (this._thinkingControl !== null) {
  createParams['thinking'] = this._thinkingControl; // { type: 'enabled' | 'disabled' }
}

if (tools.length > 0) {
  createParams['tools'] = tools.map((t) => toolToOpenAI(t));
}

if (this._stream) {
  createParams['stream_options'] = { include_usage: true };
}
```

---

## Error & Degradation Table

| Error Class | Trigger | Immediate Handling | Degradation Path | Recovery |
|---|---|---|---|---|
| `APIConnectionError` | Network failure, DNS failure, TLS error | Throw to caller; ody-code TUI shows "connection failed" toast | Retry on next user prompt (no automatic retry) | User checks network / baseUrl |
| `APITimeoutError` | Request exceeds timeout | Throw to caller | Same as above | User retries or increases timeout |
| `APIStatusError` (4xx) | Invalid API key, malformed request, model not found | Throw to caller; message contains GLM's error text | No degradation — caller must fix config | User corrects API key or model name |
| `APIStatusError` (5xx) | GLM server error | Throw to caller | No automatic retry | User retries later |
| `APIEmptyResponseError` | Empty choices array in response | Throw to caller | No degradation | Retry |

All errors are converted via `convertOpenAIError` from `openai-common.ts`, which maps OpenAI SDK errors into kosong's error taxonomy [C:USER].

---

## Test Plan

**File:** `packages/kosong/test/providers/glm.test.ts` (new)

### Test 1: Constructor resolves API key from env
```ts
const provider = new GLMChatProvider({ model: 'glm-4.7' });
expect(provider.modelName).toBe('glm-4.7');
```
- Must pass when `GLM_API_KEY` is set.
- Must pass when `apiKey` is passed explicitly.

### Test 2: getCapability returns UNKNOWN_CAPABILITY
```ts
const provider = new GLMChatProvider({ model: 'glm-4.7', apiKey: 'test' });
expect(provider.getCapability()).toBe(UNKNOWN_CAPABILITY);
```

### Test 3: withThinking maps effort levels
```ts
const provider = new GLMChatProvider({ model: 'glm-4.7', apiKey: 'test' })
  .withThinking('off');
expect(provider.thinkingEffort).toBe('off');
```

### Test 4: Message conversion filters empty text parts
```ts
const msg = createAssistantMessage([
  { type: 'text', text: '' },
  { type: 'text', text: 'hello' },
]);
const converted = convertMessage(msg, null);
// converted.content should be 'hello' (string), not [{ type: 'text', text: '' }, { type: 'text', text: 'hello' }]
```

### Test 5: generate() injects thinking param when withThinking('off')
- Mock OpenAI SDK client.
- Assert that `chat.completions.create` receives `thinking: { type: 'disabled' }`.

### Test 6: generate() omits thinking param when thinking is default
- Mock OpenAI SDK client.
- Assert that create params do NOT contain `thinking`.

### Test 7: StreamedMessage extracts reasoning_content
- Feed a mock async iterator with chunks containing `delta.reasoning_content`.
- Assert yielded parts include `{ type: 'think', think: '...' }`.

### Test 8: StreamedMessage extracts text and tool_calls
- Feed mock chunks with text deltas and tool_call deltas.
- Assert correct `StreamedMessagePart` sequence.

**Done criteria:**
```bash
pnpm --filter kosong test packages/kosong/test/providers/glm.test.ts
pnpm --filter kosong typecheck
```
Both must pass.

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | GLM API makes a breaking change to `thinking` parameter shape | Medium | High | Keep the `thinking` object shallow; if shape changes, we only change one field in `generate()`. |
| 2 | Empty text content part filter is too aggressive and drops valid parts | Low | Medium | Filter only parts where `text === ''` (exact empty string), not whitespace-only strings. |
| 3 | OpenAI SDK version drift causes incompatibility with GLM's wire format | Low | High | Pin `openai` version in `package.json`; update only after testing against real GLM endpoint. |
| 4 | `UNKNOWN_CAPABILITY` causes poor UX in model selector (no capability badges) | Medium | Low | Users can manually set `capabilities` on their ModelAlias config; document this. |
| 5 | GLM `reasoning_content` field name changes in future API versions | Low | Medium | Use the same field-scan logic as OpenAILegacyChatProvider (scan `reasoning_content`, `reasoning_details`, `reasoning`). |

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | GLM's `thinking: { type: 'enabled'\|'disabled' }` parameter is accepted by both Z.AI and bigmodel.cn endpoints. | Medium | High — reasoning control would not work. | Make a real API call to both endpoints with the parameter and verify response. |
| 2 | GLM returns `reasoning_content` in the same location as OpenAI-compatible reasoners (`message.reasoning_content` and `delta.reasoning_content`). | High | Medium — reasoning traces would be lost. | Verified from Z.AI official docs [C:UPSTREAM]; sample response shows `message.reasoning_content`. |
| 3 | Filtering out empty-string `text` content parts prevents the `messages[N].content[M].text: 不能为空` validation error without breaking valid requests. | Medium | Medium — could drop valid empty strings or fail to fix the error. | Test against real GLM endpoint with a message that previously triggered the error. |
| 4 | GLM supports `stream_options: { include_usage: true }` for per-chunk usage reporting. | Medium | Low — usage would only appear on final chunk or not at all. | Test streaming request against real GLM endpoint and inspect chunk usage fields. |
| 5 | GLM supports parallel tool calls and incremental tool_call argument streaming in the same shape as OpenAI Chat Completions. | Medium | High — tool calling would break. | Test a real tool-calling conversation with multiple tools against GLM endpoint. |
| 6 | The `openai` npm SDK handles GLM's SSE stream format without custom parser modifications. | High | High — streaming would break entirely. | Run a real streaming request through the SDK and verify chunks arrive correctly. |

---

## Design Self-Review

### Adversarial scrutiny — most expensive decisions

**Decision 1: Empty text content part filter**

This filter prevents GLM's known validation error. Three concrete inputs:
1. `[{ type: 'text', text: '' }, { type: 'text', text: 'hello' }]` → expected: content = `'hello'` (single string, empty part dropped).
2. `[{ type: 'text', text: '' }]` → expected: content = `undefined` (no parts after filtering).
3. `[{ type: 'text', text: ' ' }]` → expected: content = `' '` (whitespace-only preserved, not filtered).

Verification (ephemeral, no file write):
```bash
node -e "
const parts = [{type:'text',text:''},{type:'text',text:'hello'}];
const filtered = parts.filter(p => p.type !== 'text' || p.text !== '');
console.log(filtered.length === 1 && filtered[0].text === 'hello');
"
```

**Decision 2: Thinking parameter mapping**

`ThinkingEffort` has 6 levels (`off`, `low`, `medium`, `high`, `xhigh`, `max`). GLM only supports on/off via `thinking.type`. Mapping:
- `'off'` → `{ type: 'disabled' }`
- any other value → omit `thinking` field (default enabled)

Three concrete inputs:
1. `withThinking('off')` → expected: create params contain `thinking: { type: 'disabled' }`
2. `withThinking('medium')` → expected: create params do NOT contain `thinking`
3. default (no `withThinking` call) → expected: create params do NOT contain `thinking`

### Four-lens sweep

**Security:** API key is read from `GLM_API_KEY` env var or explicit config, same pattern as all other providers. No PII leakage risk beyond what already exists in the generic provider auth flow. The empty-text filter operates on message content — no secrets involved.

**Test:** Every behaviour above has a must-pass case in the test plan. Test 4 asserts the empty-text filter; Test 5 asserts thinking param injection; Test 7 asserts reasoning_content extraction. No assertion contradicts its own constants.

**Ops:** No new external service dependencies beyond the existing `openai` npm package. GLM endpoint latency is comparable to other cloud LLM APIs. No identifier collision: `glm` provider type is new and distinct.

**Integration:** Every data source/hook verified:
- `ChatProvider` interface exists in `packages/kosong/src/provider.ts` [C:VERIFIED]
- `createProvider` switch exists in `packages/kosong/src/providers/index.ts` [C:VERIFIED]
- `convertOpenAIError` exists in `packages/kosong/src/providers/openai-common.ts` [C:VERIFIED]
- `extractUsage` exists in `packages/kosong/src/providers/openai-common.ts` [C:VERIFIED]
- `normalizeOpenAIFinishReason` exists in `packages/kosong/src/providers/openai-common.ts` [C:VERIFIED]
- `convertChatCompletionStreamToolCall` exists in `packages/kosong/src/providers/chat-completions-stream.ts` [C:VERIFIED]
- `toolToOpenAI` exists in `packages/kosong/src/providers/openai-common.ts` [C:VERIFIED]
- `UNKNOWN_CAPABILITY` exists in `packages/kosong/src/capability.ts` [C:VERIFIED]

---

## Consolidated Audit Gate (Deep)

Please confirm or correct each numbered item:

1. **Architecture:** Standalone `GLMChatProvider` using OpenAI SDK for transport, custom message/response translation. [C:USER] [C:INFERRED]
2. **Endpoint:** Default `https://api.z.ai/api/paas/v4/`, overridable via `baseUrl`. [C:USER]
3. **Auth:** API key only, env var `GLM_API_KEY` or explicit `apiKey` option. [C:USER]
4. **Capability:** Returns `UNKNOWN_CAPABILITY` for all models (no hard-coded catalog). [C:USER]
5. **Thinking:** Maps `ThinkingEffort` to GLM `thinking: { type: 'enabled'|'disabled' }`; only `'off'` disables, all other values default to enabled. [C:INFERRED]
6. **Tool calls:** Full support via OpenAI SDK + `convertChatCompletionStreamToolCall`. [C:USER]
7. **Empty text filter:** Drops content parts where `text === ''` before serialization. [C:INFERRED]
8. **Error handling:** Reuses `convertOpenAIError` from `openai-common.ts`. [C:USER]
9. **Scope Out:** OAuth, vision/multimodal, `clear_thinking` control, custom SSE parser. [C:USER] [C:DEFERRED]
10. **Assumption 1 (Medium confidence):** GLM `thinking` parameter accepted by both endpoints — needs real API verification. [C:INFERRED]
11. **Assumption 3 (Medium confidence):** Empty-text filter fixes validation error without breaking valid requests — needs real API verification. [C:INFERRED]
