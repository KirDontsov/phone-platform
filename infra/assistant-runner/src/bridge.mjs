import crypto from "node:crypto";
import { pathToFileURL } from "node:url";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  ListToolsRequestSchema,
  CallToolRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { AjvJsonSchemaValidator } from "@modelcontextprotocol/sdk/validation/ajv";
import { TOOLS, apiPost, requireValue, secret, safeCode } from "./common.mjs";
export function makeBridge({ apiUrl, capability, post = apiPost }) {
  secret(capability);
  const server = new Server(
    { name: "phone-platform-configuration", version: "1.0.0" },
    { capabilities: { tools: {} } },
  );
  let catalogue = null;
  let invocations = 0;
  const validators = new Map();
  const validator = new AjvJsonSchemaValidator();
  async function load() {
    if (catalogue) return catalogue;
    const body = await post(apiUrl, "/api/assistant/tools", capability, {
      method: "list",
    });
    requireValue(
      Array.isArray(body.tools) && body.tools.length === TOOLS.length,
      "invalid_tool_catalogue",
    );
    const seen = new Set();
    for (const tool of body.tools) {
      requireValue(
        tool &&
          TOOLS.includes(tool.name) &&
          !seen.has(tool.name) &&
          typeof tool.description === "string" &&
          tool.description.length <= 4000 &&
          tool.inputSchema?.type === "object" &&
          tool.inputSchema.additionalProperties === false,
        "invalid_tool_catalogue",
      );
      seen.add(tool.name);
      requireValue(
        Buffer.byteLength(JSON.stringify(tool.inputSchema)) <= 32768,
        "invalid_tool_schema",
      );
      validators.set(tool.name, validator.getValidator(tool.inputSchema));
    }
    catalogue = body.tools.map(({ name, description, inputSchema }) => ({
      name,
      description,
      inputSchema,
    }));
    return catalogue;
  }
  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: await load(),
  }));
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    try {
      requireValue(++invocations <= 24, "assistant_tool_budget_reached");
      requireValue(TOOLS.includes(request.params.name), "unsupported_tool");
      await load();
      const args = request.params.arguments ?? {};
      requireValue(
        Buffer.byteLength(JSON.stringify(args)) <= 32768 &&
          validators.get(request.params.name)(args).valid,
        "invalid_tool_arguments",
      );
      const call_id = crypto.randomUUID();
      const result = await post(
        apiUrl,
        "/api/assistant/tools",
        capability,
        { method: "call", call_id, name: request.params.name, arguments: args },
        { timeout: 25000 },
      );
      requireValue(
        result && typeof result.result === "object" && result.result !== null,
        "invalid_tool_result",
      );
      const serialized = JSON.stringify(result.result);
      requireValue(
        Buffer.byteLength(serialized) <= 262144,
        "tool_result_too_large",
      );
      return { content: [{ type: "text", text: serialized }] };
    } catch (error) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: JSON.stringify({
              error: { code: safeCode(error, "assistant_tool_failed") },
            }),
          },
        ],
      };
    }
  });
  return server;
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  process.umask(0o077);
  try {
    const server = makeBridge({
      apiUrl: process.env.PHONE_ASSISTANT_API_URL,
      capability: process.env.PHONE_ASSISTANT_CAPABILITY,
    });
    await server.connect(new StdioServerTransport());
  } catch {
    process.stderr.write("assistant_mcp_start_failed\n");
    process.exitCode = 1;
  }
}
