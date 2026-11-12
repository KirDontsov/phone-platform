import test from "node:test";
import assert from "node:assert/strict";
import { nativeProbe } from "./native-probe.mjs";
test("actual pinned Codex plus real MCP SDK: offline provider denies OS tools and retains only service authority", async () => {
  const report = await nativeProbe();
  assert.equal(report.actual_sdk_mcp_call, true);
  assert.equal(report.forbidden_exec_rejected, true);
  assert.equal(report.external_model_requests, 0);
});
test("actual pinned Codex completes through production createRunner HTTP and TurnService with the full startup/turn parameters", async () => {
  const report = await nativeProbe({ throughRunner: true });
  assert.equal(report.production_orchestrator, true);
  assert.equal(report.actual_sdk_mcp_call, true);
  assert.equal(report.forbidden_exec_rejected, true);
  assert.equal(report.external_model_requests, 0);
});
test("actual pinned Codex with production gpt-6.1-sol search-capable catalogue exposes eight direct service tools without code/OS execution", async () => {
  const report = await nativeProbe({
    throughRunner: true,
    searchCapableModel: true,
  });
  assert.equal(report.search_capable_model, true);
  assert.equal(report.direct_service_tool_count, 8);
  assert.equal(report.actual_sdk_mcp_call, true);
  assert.equal(report.forbidden_exec_rejected, true);
  assert.equal(report.external_model_requests, 0);
});
