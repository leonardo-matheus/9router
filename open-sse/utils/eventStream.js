/**
 * Amazon EventStream decoder for Bedrock Converse Stream responses.
 *
 * EventStream is a binary framing protocol. Each message is:
 *   [4 bytes totalLen][4 bytes headersLen][4 bytes preludeCrc][headers][payload][4 bytes msgCrc]
 *
 * We only care about the JSON payloads. This decoder yields the JSON objects
 * found in the `:message-type=event` messages.
 */

/**
 * Parse a single EventStream message from a buffer.
 * Returns { headers, payload, bytesConsumed } or null if incomplete.
 */
function parseMessage(buf, offset = 0) {
  if (buf.length - offset < 12) return null; // need at least prelude

  const totalLen = buf.readUInt32BE(offset);
  if (buf.length - offset < totalLen) return null; // incomplete message

  const headersLen = buf.readUInt32BE(offset + 4);
  // prelude CRC at offset+8 (skip validation for perf)
  const headersStart = offset + 12;
  const payloadStart = headersStart + headersLen;
  const payloadLen = totalLen - 12 - headersLen - 4; // subtract prelude(12) + headers + msgCrc(4)

  const payload = buf.slice(payloadStart, payloadStart + payloadLen);
  return { payload, bytesConsumed: totalLen };
}

/**
 * Decode an EventStream binary buffer into an async iterator of parsed JSON objects.
 * Handles chunked data from a ReadableStream.
 */
export function decodeEventStream(reader) {
  return {
    [Symbol.asyncIterator]() {
      let leftover = Buffer.alloc(0);
      let done = false;

      return {
        async next() {
          while (!done) {
            // Try to parse from leftover buffer
            if (leftover.length >= 12) {
              const totalLen = leftover.readUInt32BE(0);
              if (leftover.length >= totalLen) {
                const msg = parseMessage(leftover, 0);
                if (msg) {
                  leftover = leftover.slice(msg.bytesConsumed);
                  // Try to parse JSON payload
                  try {
                    const json = JSON.parse(msg.payload.toString("utf8"));
                    // Filter for event-type messages
                    if (json.delta || json.message || json.stopReason || json.usage || json.metrics) {
                      return { value: json, done: false };
                    }
                    // Skip non-event messages (like prelude/padding)
                    continue;
                  } catch {
                    continue; // skip non-JSON payloads
                  }
                }
              }
            }

            // Read more data
            const { value, done: streamDone } = await reader.read();
            if (streamDone) {
              done = true;
              return { done: true };
            }
            // value is a Uint8Array from the reader
            leftover = Buffer.concat([leftover, Buffer.from(value)]);
          }
          return { done: true };
        }
      };
    }
  };
}
