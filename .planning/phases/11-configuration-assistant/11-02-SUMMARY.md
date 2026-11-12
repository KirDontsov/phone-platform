---
phase: "11-configuration-assistant"
plan: "02"
status: implemented_local_verification
requirements: [CODEX-01, CODEX-02, MCP-01]
requirements-completed: []
verified_at: "2026-11-12"
external_acceptance: pending
---

# 11-02: Docker Codex и MCP bridge

Отдельный nonroot Docker assistant-runner, pinned Codex0.167.2/MCP SDK1.32.2/Node24, private8091 без hostport. Fixed app-server RPC wrapper, ephemeral thread/home/workspace, запрещённые builtin tools выключены; нет DB/repo/media/Docker socket/host auth. Собственная авторизация UI apiKey/device code, staged home→API authorization→canonical promotion; поколения/cancel/expiry/process-group cleanup. Auth volume вне backup, restore требует нового входа.

Настоящий MCP SDK stdio bridge предоставляет ровно8tools; scoped capability не попадает в prompt/argv/history. API gateway fixed, произвольные HTTP/SQL/OS/IAM недоступны. NDJSON bounded/single-terminal, безопасные коды ошибок и частичный текст.

Mapping: infra/assistant-runner/{Dockerfile,package*.json,src,tests}, infra/compose.yml, scripts/setup-local.py, scripts/dev.sh, docs/assistant-runner.md.

15/15 Node/Docker tests PASS. Реальный pinned Codex + production config + локальный Responses fixture вызывает разрешённый MCP get_workspace_configuration через настоящий SDK; forced exec_command отвергнут, marker отсутствует. Внешних model requests0. Fake app-server tests проверяют прочие lifecycle/negative cases отдельно. Наличие конфигурационных флагов само по себе не считалось executor proof.

API key/device approval с реальным аккаунтом, model entitlement/качество и реальные Телфин credentials — live gates Pending. Сервис без авторизации честно требует вход, demo-ответ модели отсутствует.

Research →3 plans → independent plan-check →revision1/3 PASS →execute → independent implementation review/fixes → verifier → automated UAT. Plan-check оценивает план; результаты реализации записываются отдельно. Общая приёмка не помечается завершённой без пользовательского теста. [План](11-02-PLAN.md), [контракт](../../../docs/contracts/assistant-v1.md), [независимый review](../../reviews/11-RUNNER-API-REVIEW.md).
