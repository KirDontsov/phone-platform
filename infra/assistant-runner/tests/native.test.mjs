import test from "node:test";
import assert from "node:assert/strict";
import { nativeProbe } from "./native-probe.mjs";
test("actual pinned Codex plus real MCP SDK: offline provider denies OS tools and retains only service authority", async () => {
  const report = await nativeProbe();
  assert.equal(report.actual_sdk_mcp_call, true);
  assert.equal(report.forbidden_exec_rejected, true);
  assert.equal(report.external_model_requests, 0);
});
