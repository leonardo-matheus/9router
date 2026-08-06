/**
 * AWS SigV4 request signing utility for Bedrock API.
 * Implemented as a pure function (no external deps) so it works in any Node.js runtime.
 *
 * Reference: https://docs.aws.amazon.com/IAM/latest/GeneralReference/CreateSignedUpURL.html
 */
import crypto from "node:crypto";

const AWS_ALGORITHM = "AWS4-HMAC-SHA256";

function hashHex(data) {
  return crypto.createHash("sha256").update(data, "utf8").digest("hex");
}

function hmac(key, data) {
  return crypto.createHmac("sha256", key).update(data, "utf8").digest();
}

function getSigningKey(secretKey, date, region, service) {
  let k = Buffer.from(`AWS4${secretKey}`, "utf8");
  k = hmac(k, date);
  k = hmac(k, region);
  k = hmac(k, service);
  k = hmac(k, "aws4_request");
  return k;
}

function canonicalHeaders(headers) {
  const entries = Object.entries(headers)
    .map(([k, v]) => [k.toLowerCase().trim(), String(v).trim()])
    .sort((a, b) => (a[0] < b[0] ? -1 : 1));
  const grouped = {};
  for (const [k, v] of entries) {
    if (grouped[k]) grouped[k] += `,${v}`;
    else grouped[k] = v;
  }
  const keys = Object.keys(grouped).sort();
  const pairs = keys.map((k) => `${k}:${grouped[k]}`);
  return {
    signedHeaders: keys.join(";"),
    headerString: pairs.join("\n") + "\n",
  };
}

/**
 * Sign a request for AWS Bedrock (or any AWS service) using SigV4.
 *
 * @param {object} opts
 * @param {string} opts.method      HTTP method (POST, GET, etc.)
 * @param {string} opts.url        Full URL (e.g. https://bedrock-runtime.us-east-1.amazonaws.com/...)
 * @param {object} opts.headers     Outgoing headers (mutated in place — signed headers added/overwritten)
 * @param {string} opts.body       Raw request body string (already serialised)
 * @param {string} opts.accessKeyId  AWS Access Key ID
 * @param {string} opts.secretAccessKey AWS Secret Access Key
 * @param {string} opts.region     AWS region (e.g. "us-east-1")
 * @param {string} [opts.service="bedrock"] AWS service name
 * @param {string} [opts.sessionToken] AWS session token (optional, for STS/IAM roles)
 * @returns {object} The signed headers object
 */
export function signRequest({
  method,
  url,
  headers = {},
  body = "",
  accessKeyId,
  secretAccessKey,
  region,
  service = "bedrock",
  sessionToken,
}) {
  if (!accessKeyId || !secretAccessKey || !region) {
    throw new Error("AWS SigV4: accessKeyId, secretAccessKey, and region are required");
  }

  const parsedUrl = new URL(url);
  const host = parsedUrl.host;
  const path = parsedUrl.pathname || "/";
  const query = parsedUrl.search;

  const now = new Date();
  const amzDate = now.toISOString().replace(/[:-]|\.\d+/g, ""); // YYYYMMDDTHHMMSSZ
  const dateStamp = amzDate.slice(0, 8); // YYYYMMDD

  // Build canonical headers — always include host
  const canonical = canonicalHeaders({
    host,
    "x-amz-content-sha256": hashHex(body),
    "x-amz-date": amzDate,
    ...(sessionToken ? { "x-amz-security-token": sessionToken } : null),
    ...headers,
  });

  // Canonical query string
  const canonicalQuery = query
    .slice(1) // remove leading ?
    .split("&")
    .filter(Boolean)
    .map((p) => {
      const [k, v = ""] = p.split("=");
      return `${encodeURIComponent(k)}=${encodeURIComponent(v)}`;
    })
    .sort()
    .join("&");

  const canonicalRequest = [
    method.toUpperCase(),
    path,
    canonicalQuery,
    canonical.headerString,
    canonical.signedHeaders,
    hashHex(body),
  ].join("\n");

  const credentialScope = `${dateStamp}/${region}/${service}/aws4_request`;
  const stringToSign = [
    AWS_ALGORITHM,
    amzDate,
    credentialScope,
    hashHex(canonicalRequest),
  ].join("\n");

  const signingKey = getSigningKey(secretAccessKey, dateStamp, region, service);
  const signature = crypto
    .createHmac("sha256", signingKey)
    .update(stringToSign, "utf8")
    .digest("hex");

  // Merge signed headers into the outgoing headers
  const signedHeaders = {
    ...headers,
    host,
    "x-amz-content-sha256": hashHex(body),
    "x-amz-date": amzDate,
    Authorization:
      `${AWS_ALGORITHM} Credential=${accessKeyId}/${credentialScope}, ` +
      `SignedHeaders=${canonical.signedHeaders}, Signature=${signature}`,
  };
  if (sessionToken) signedHeaders["x-amz-security-token"] = sessionToken;

  return signedHeaders;
}
