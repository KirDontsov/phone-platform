---
phase: "02-platform-auth"
plan: "03"
status: local_verified_user_acceptance_pending
requirements-completed: []
verified_at: "2026-11-11"
external_acceptance: pending
---

# 02-03: Schema/contracts/durable jobs

Реализация существует и проверена в указанном локальном scope. Эта запись не закрывает live-требования PLAT-01 и не заменяет пользовательский UAT.

## Результат и mapping

Фактические артефакты: [`crates/contracts/src/lib.rs`](../../../crates/contracts/src/lib.rs), [`crates/persistence/src/lib.rs`](../../../crates/persistence/src/lib.rs), [`crates/persistence/migrations/0001_access/up.sql`](../../../crates/persistence/migrations/0001_access/up.sql), [`crates/persistence/tests/postgres.rs`](../../../crates/persistence/tests/postgres.rs), [`infra/compose.yml`](../../../infra/compose.yml).

Отклонение от filenames/объёма исходного [02-03-PLAN.md](02-03-PLAN.md): Вместо миграций0002…0012 и entity-specific repositories используется tenant documents(JSONB)+typed domain validation. Jobs/outbox отдельные таблицы. Миграции embedded/versioned/advisory-locked; jobs.rs и migrations.rs объединены в lib.rs/PGtest. smoke-compose.sh/compose.test.yml заменены scripts/smoke-compose.py с отдельным random project; down.sql не создан (миграции вперёд).

## Фактическая проверка

- 15 core unit tests и 1 отдельный реальный PG persistence test PASS.
- 2 Actix HTTP/PG integration tests PASS: session/reset/CSRF/RBAC/tenant isolation/optimistic updates/secrets.
- Frontend build +20 unit PASS; Docker API/worker/PG/Mailpit E2E13/13 без skips (44.1s, workers1); после accessaudit/nginx DNS — affected invitation/reprocess2/2 PASS (14.8s), без увеличения distinct count.

Общий реестр команд и границ evidence: [EXECUTION-DELTA](../../research/EXECUTION-DELTA.md). Проверка именно этой фазы: [02-VERIFICATION.md](02-VERIFICATION.md). Не запускался каждый старый plan-specific command с отсутствующим test filename: его фактический suite указан в mapping.

## Приёмка и provenance

- Пользовательский UAT; production SMTP/TLS/секреты на выбранном сервере.

Source commits текущего цикла: `e789168` platform, `7af83b7` ops/evaluation, `3563c4f` access audit/nginx DNS. Research baseline: [02-RESEARCH.md](02-RESEARCH.md); contract delta: [platform-v1](../../../docs/contracts/platform-v1.md); baseline plan-check и две итерации: [PLAN-CHECK](../../PLAN-CHECK.md), [REVISION-LOG](../../REVISION-LOG.md). Эти planning checks не выдаются за verification реализации. Независимый [cross-phase review](../../reviews/FINAL-VERIFICATION.md) выполнен; [02-UAT](02-UAT.md) остаётся открытым.
