# 11 — Configuration Assistant: research

2026-11-12, до реализации. Пользователь явно расширил продукт: чат как Bondscan, Codex в Docker, MCP tools для самостоятельной настройки Телфина/статусов/тегов/фильтров. Работа продолжается автономно; credentials/model inference нельзя выдумывать.

Read-only образец: `../obligation-platform/obligation-client/src/features/ai-chat/ui/{AiChatPanel,AiChatFab,ChatComposer,ToolUseChips}.vue`, store/api и `obligation-api/src/services/ai_chat_opencode.rs`, `src/bin/ai_chat_mcp_server.rs`, `ai_chat_mcp_token.rs`. Полезны FAB/fullpage/history/progress, per-turn identity. Его DB credentials в subprocess/user-only claims не переносятся. Ключи/.env/auth.json не читались.

Официальные источники OpenAI Docs прочитаны до выбора runtime:

- [Codex app-server](https://learn.chatgpt.com/docs/app-server): JSONL initialize, ephemeral threads, turns/notifications, account/login/start apiKey и chatgptDeviceCode.
- [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli): stdio/HTTP MCP, env/token configuration, enabled tools.
- [Config reference](https://learn.chatgpt.com/docs/config-file/config-reference): features/sandbox/permissions; не считать один prompt безопасной границей.
- [Noninteractive](https://learn.chatgpt.com/docs/non-interactive-mode): JSONL exec и auth caveats; app-server выбран ради login UX и строгого контроля RPC lifecycle.

Фактические версии npm: @openai/codex0.167.2, @modelcontextprotocol/sdk1.32.2. Runner researcher проверил pinned CLI в новом временном CODEX_HOME, без личной auth и платных запросов: version/schema/strict config, initialize/account-read null, ephemeral thread pathnull, environments[]/runtimeWorkspaceRoots[], readOnly sandbox/networkfalse. Host RPC thread/shellCommand unsandboxed — его и любые arbitrary RPC запрещает wrapper. Effective builtin capabilities ещё должны получить fail-closed implementation/tests; flags сами по себе не являются доказательством.

Решение: отдельный assistant-runner, реальные stdio MCP tools через scoped API gateway. Анализатор аудио/текста остаётся MCP0. Подключение ключей — inline secure card; модель получает только handle. API runtime role/session/org проверки для каждого tool, mutations используют общие сервисы и atomic receipt/audit. Chat history приложения — БД, каждый turn fresh ephemeral Codex context, без reuse чужих sessions.

Детальный контракт: `docs/contracts/assistant-v1.md`; UX research: `docs/assistant-ui.md`. Plan-checker отдельно проверяет identity/secret/authority/idempotency/cancel/versioning/restore boundaries. Без авторизации Codex UI показывает необходимость подключения, не отвечает учебным текстом под видом модели. Device login требует реального действия владельца/разрешений его аккаунта; это external acceptance, не повод останавливать независимую реализацию.
