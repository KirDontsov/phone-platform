---
phase: "11-configuration-assistant"
plan: "01"
status: implemented_local_verification
requirements: [CHAT-01, CHAT-02, CHAT-03, MCP-01, SETUP-01]
requirements-completed: []
verified_at: "2026-11-12"
external_acceptance: pending
---

# 11-01: Scoped chat API и настройки

Сохранённые conversations/messages/runs с порядком sequence и durable partial updates; owner/user/org guards, CSRF, idempotency, ограничение истории/контекста, cancel/delete/recovery. Секреты Телфина AES-GCM, refs на24h, consume ciphertext +minute GC, deletion ledger. Своя авторизация Codex с staged callback, исходной сессией и поколениями.

Восемь MCP tools используют shared helpers HTTP settings, общий organization revision и версии объектов. Мутация/receipt/audit атомарны; failed actions также расходуют budget и получают replayable receipt. Единый порядок org→conversation→run; organization lock FOR NO KEY UPDATE не конфликтует с FK KEY SHARE. Проверка Телфина вне DB transaction с повторной проверкой authority/version.

Mapping: phone-api/src/assistant.rs; assistant/{store,model_auth,runtime,tools}.rs; settings.rs; tests/assistant.rs; scripts/backup.py и scripts/tests/test_backup_tools.py. Отдельные modules вместо одного большого assistant.rs; новые document kinds используют существующую tenant-схему, новая миграция не нужна.

Actual Actix/loopback runner/PostgreSQL6/6 PASS: scope/privacy/replay/CAS/config, concurrent revoke/cancel/switch/role loss, delayed auth generations, partial/EOF/protocol failures, physical maintenance. Shared HTTP platform2/2 PASS на новой disposable DB. Backup17/17 PASS, включая2actualPG: deleted chats/credentials не воскрешаются, restored auth/capabilities отозваны, временный ciphertext удалён. Последующие найденные verifier замечания и финальные повторы — в VERIFICATION.

Research →3 plans → independent plan-check →revision1/3 PASS →execute → independent implementation review/fixes → verifier → automated UAT. Plan-check оценивает план; результаты реализации записываются отдельно. Общая приёмка не помечается завершённой без пользовательского теста. [План](11-01-PLAN.md), [контракт](../../../docs/contracts/assistant-v1.md), [независимый review](../../reviews/11-RUNNER-API-REVIEW.md).
