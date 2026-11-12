# Pinned Codex model metadata

Source: https://github.com/openai/codex/blob/rust-v0.167.2/codex-rs/models-manager/models.json
Upstream SHA256: 1384625c0b6b3e67d3c60da29becea2b017f2c283fc1e2d05b8edafb4698eb10
License: Apache-2.0, LICENSE.codex.

The capability policy sets every model's tool_mode to direct, apply_patch_tool_type to null and experimental_supported_tools to an empty list. Model identifiers, reasoning and context limits, and all remaining metadata are preserved. Core0.167.2 otherwise gives model tool_mode precedence over disabled code_mode feature flags and can advertise a generic JavaScript exec tool. This application permits the eight typed MCP tools directly and requires native forced-execution tests after any CLI/catalog update. The catalog is immutable application code, contains no credentials and does not grant model entitlement or alter account authorization.
