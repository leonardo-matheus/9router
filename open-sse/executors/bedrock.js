import { BaseExecutor } from "./base.js";
import { proxyAwareFetch } from "../utils/proxyFetch.js";
import { FORMATS } from "../translator/formats.js";

/**
 * Detect whether a connection targets AWS Bedrock.
 */
function isBedrockConnection(credentials) {
  const psd = credentials?.providerSpecificData;
  if (psd?.prefix === "bedrock" || psd?.baseUrl?.includes("bedrock-runtime")) return true;
  if (psd?.nodeName?.toLowerCase().includes("bedrock")) return true;
  return false;
}

/**
 * Parse the ABSK token to extract the raw Bearer token value.
 * The ABSK token IS the bearer token itself — we just strip the "ABSK" prefix
 * and decode the base64 payload.
 */
function parseBedrockBearerToken(apiKey) {
  if (!apiKey || typeof apiKey !== "string") return null;
  // The ABSK token is the bearer token. Strip the ABSK prefix to get the
  // base64-encoded payload, then decode to get the actual credential string.
  // But Bedrock accepts the full ABSK string as the Bearer token directly!
  return apiKey;
}

/**
 * Transform an OpenAI-format messages array into Bedrock Converse format.
 *
 * OpenAI format:
 *   { messages: [{ role: "system"|"user"|"assistant", content: string|[{type,text}...] }] }
 *
 * Bedrock Converse format:
 *   { messages: [{ role: "user"|"assistant", content: [{ text: "..." }] }],
 *     system: [{ text: "..." }] }
 */
function openaiToConverse(body) {
  const messages = body.messages || [];
  const systemPrompts = [];
  const converseMessages = [];

  for (const msg of messages) {
    const role = msg.role;
    let textContent = "";

    if (typeof msg.content === "string") {
      textContent = msg.content;
    } else if (Array.isArray(msg.content)) {
      // Flatten content blocks to text
      textContent = msg.content
        .filter((c) => c.type === "text")
        .map((c) => c.text)
        .join("\n");
    }

    if (role === "system") {
      systemPrompts.push({ text: textContent });
    } else if (role === "user") {
      converseMessages.push({
        role: "user",
        content: [{ text: textContent }],
      });
    } else if (role === "assistant") {
      converseMessages.push({
        role: "assistant",
        content: [{ text: textContent }],
      });
    }
  }

  // Bedrock Converse requires at least one user message
  if (converseMessages.length === 0) {
    converseMessages.push({ role: "user", content: [{ text: "Hello" }] });
  }

  const converseBody = {
    messages: converseMessages,
    inferenceConfig: {},
  };

  if (systemPrompts.length > 0) {
    converseBody.system = systemPrompts;
  }

  if (body.max_tokens) converseBody.inferenceConfig.maxTokens = body.max_tokens;
  if (body.temperature !== undefined) converseBody.inferenceConfig.temperature = body.temperature;
  if (body.top_p !== undefined) converseBody.inferenceConfig.topP = body.top_p;
  if (body.stop) {
    converseBody.inferenceConfig.stopSequences = Array.isArray(body.stop)
      ? body.stop
      : [body.stop];
  }

  return converseBody;
}

/**
 * Convert a Bedrock Converse response back to OpenAI chat completions format.
 */
function converseToOpenai(converseResponse, model) {
  const output = converseResponse.output;
  const message = output?.message;
  const content = message?.content || [];
  const textParts = content.filter((c) => c.text).map((c) => c.text);
  const usage = converseResponse.usage || {};

  return {
    id: `chatcmpl-bedrock-${Date.now()}`,
    object: "chat.completion",
    created: Math.floor(Date.now() / 1000),
    model: model,
    choices: [
      {
        index: 0,
        message: {
          role: "assistant",
          content: textParts.join(""),
        },
        finish_reason: mapStopReason(converseResponse.stopReason),
      },
    ],
    usage: {
      prompt_tokens: usage.inputTokens || 0,
      completion_tokens: usage.outputTokens || 0,
      total_tokens: (usage.inputTokens || 0) + (usage.outputTokens || 0),
    },
  };
}

function mapStopReason(reason) {
  switch (reason) {
    case "end_turn":
    case "stop_sequence":
      return "stop";
    case "max_tokens":
      return "length";
    case "content_filter":
      return "content_filter";
    default:
      return "stop";
  }
}

/**
 * BedrockExecutor — calls AWS Bedrock via the Converse API using Bearer token auth.
 *
 * The ABSK token (stored in apiKey) is used directly as a Bearer token.
 * The request body is transformed from OpenAI chat completions format to
 * Bedrock Converse format, and the response is transformed back.
 *
 * For streaming, Bedrock returns Amazon EventStream (binary), which this
 * executor converts to SSE (text/event-stream) format.
 */
export class BedrockExecutor extends BaseExecutor {
  constructor() {
    super("bedrock");
  }

  buildUrl(model, stream, urlIndex = 0, credentials = null) {
    const region = credentials?.providerSpecificData?.region || "us-east-1";
    const baseUrl =
      credentials?.providerSpecificData?.baseUrl ||
      `https://bedrock-runtime.${region}.amazonaws.com`;
    const normalized = baseUrl.replace(/\/$/, "");
    // Use the model ID from the connection's defaultModel or the request body
    const modelId = credentials?.defaultModel || model;
    const action = stream ? "converse-stream" : "converse";
    return `${normalized}/model/${modelId}/${action}`;
  }

  buildHeaders(credentials, stream = true) {
    const token = parseBedrockBearerToken(credentials.apiKey);
    return {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    };
  }

  transformRequest(model, body) {
    return openaiToConverse(body);
  }

  /**
   * Override execute() to handle Bedrock-specific response format.
   * For non-streaming: parse the Converse response and convert to OpenAI format.
   * For streaming: intercept the EventStream and convert to SSE format.
   */
  async execute({ model, body, stream, credentials, signal, log, proxyOptions = null }) {
    const url = this.buildUrl(model, stream, 0, credentials);
    const headers = this.buildHeaders(credentials, stream);
    const converseBody = this.transformRequest(model, body);

    if (!stream) {
      // Non-streaming: make the request and transform the response
      const response = await proxyAwareFetch(
        url,
        {
          method: "POST",
          headers,
          body: JSON.stringify(converseBody),
          signal,
        },
        proxyOptions
      );

      // If successful, read the response and transform to OpenAI format
      if (response.ok) {
        try {
          const converseResponse = await response.json();
          const modelId = credentials?.defaultModel || model;
          const openaiResponse = converseToOpenai(converseResponse, modelId);
          console.log("[BEDROCK DEBUG] converseResponse keys:", Object.keys(converseResponse));
          console.log("[BEDROCK DEBUG] openaiResponse.content:", openaiResponse.choices?.[0]?.message?.content?.slice(0, 100));
          console.log("[BEDROCK DEBUG] openaiResponse.usage:", JSON.stringify(openaiResponse.usage));

          // Create a synthetic Response with the OpenAI-formatted body
          const syntheticResponse = new Response(JSON.stringify(openaiResponse), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          });

          return {
            response: syntheticResponse,
            url,
            headers,
            transformedBody: converseBody,
            responseFormat: FORMATS.OPENAI,
          };
        } catch (err) {
          log?.error?.("BEDROCK", `Failed to parse Converse response: ${err.message}`);
        }
      }

      // Error case: return the raw response
      return { response, url, headers, transformedBody: converseBody, responseFormat: FORMATS.OPENAI };
    }

    // Streaming: intercept the response and convert EventStream to SSE
    const response = await proxyAwareFetch(
      url,
      {
        method: "POST",
        headers,
        body: JSON.stringify(converseBody),
        signal,
      },
      proxyOptions
    );

    if (!response.ok) {
      return { response, url, headers, transformedBody: converseBody };
    }

    // Convert the EventStream response to SSE format
    const modelId = credentials?.defaultModel || model;
    const sseStream = convertEventStreamToSSE(response.body, modelId);

    const sseResponse = new Response(sseStream, {
      status: 200,
      headers: {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      },
    });

    return {
      response: sseResponse,
      url,
      headers,
      transformedBody: converseBody,
      responseFormat: FORMATS.OPENAI,
    };
  }
}

/**
 * Convert a Bedrock EventStream response body to SSE format.
 *
 * EventStream is a binary protocol with frames containing:
 *   [4B totalLen][4B headersLen][4B preludeCrc][headers][payload][4B msgCrc]
 *
 * Each frame's payload is JSON with fields like:
 *   { delta: { text: "..." } }           — content chunk
 *   { stopReason: "end_turn" }           — stop signal
 *   { usage: { inputTokens, outputTokens } } — metadata
 *
 * We convert these to SSE format:
 *   data: {"choices":[{"delta":{"content":"..."}}]}\n\n
 *   data: {"choices":[{"finish_reason":"stop","delta":{}}]}\n\n
 *   data: [DONE]\n\n
 */
function convertEventStreamToSSE(responseBody, model) {
  const reader = responseBody.getReader();
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();
  let leftover = Buffer.alloc(0);

  return new ReadableStream({
    async pull(controller) {
      try {
        while (true) {
          // Try to parse from leftover buffer
          while (leftover.length >= 12) {
            const totalLen = leftover.readUInt32BE(0);
            const headersLen = leftover.readUInt32BE(4);

            if (leftover.length < totalLen) break; // incomplete frame

            const headersStart = 12;
            const payloadStart = headersStart + headersLen;
            const payloadLen = totalLen - 12 - headersLen - 4;

            if (payloadLen > 0) {
              const payloadStr = leftover
                .slice(payloadStart, payloadStart + payloadLen)
                .toString("utf8");

              try {
                const event = JSON.parse(payloadStr);

                // Convert content deltas to SSE
                if (event.delta?.text) {
                  const sseData = JSON.stringify({
                    id: `chatcmpl-bedrock-${Date.now()}`,
                    object: "chat.completion.chunk",
                    created: Math.floor(Date.now() / 1000),
                    model,
                    choices: [
                      {
                        index: 0,
                        delta: { content: event.delta.text },
                        finish_reason: null,
                      },
                    ],
                  });
                  controller.enqueue(
                    encoder.encode(`data: ${sseData}\n\n`)
                  );
                }

                // Convert stop signal to SSE
                if (event.stopReason) {
                  const finishReason = mapStopReason(event.stopReason);
                  const sseData = JSON.stringify({
                    id: `chatcmpl-bedrock-${Date.now()}`,
                    object: "chat.completion.chunk",
                    created: Math.floor(Date.now() / 1000),
                    model,
                    choices: [
                      {
                        index: 0,
                        delta: {},
                        finish_reason: finishReason,
                      },
                    ],
                  });
                  controller.enqueue(
                    encoder.encode(`data: ${sseData}\n\ndata: [DONE]\n\n`)
                  );
                  controller.close();
                  return;
                }
              } catch {
                // Skip non-JSON payloads
              }
            }

            leftover = leftover.slice(totalLen);
          }

          // Read more data
          const { value, done } = await reader.read();
          if (done) {
            controller.enqueue(encoder.encode("data: [DONE]\n\n"));
            controller.close();
            return;
          }
          leftover = Buffer.concat([leftover, Buffer.from(value)]);
        }
      } catch (err) {
        controller.error(err);
      }
    },
  });
}

export default BedrockExecutor;
