---
phase: "10-pilot-release"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-11-11
---

# Phase 10: Validation Strategy

Реализация отсутствует; команды целевые, не запущены. До execute актуализировать контракты/scaffold, измерить latency checks и завершить Wave 0. Nyquist compliance пока не заявлена.

## Test Infrastructure

Rust cargo focused/domain/integration tests в изолированных DB/broker/storage; Vue TypeScript/build/Vitest/Playwright по соответствующим UI действиям; внешние adapters по sanitized fixtures и отдельному live evidence.

## Per-Task Verification Map

| Task ID | Requirement | Automated command | Status |
|---|---|---|---|
| 10-01-1 | OPS-03 | cargo test -p phone-worker --test retention_purge | pending, file not yet created |
| 10-01-2 | OPS-03 | bash -n scripts/backup-platform.sh scripts/restore-platform.sh | pending, file not yet created |
| 10-02-1 | REL-01 | npm --prefix phone-client run test:e2e -- full-call-flow | pending, file not yet created |
| 10-02-2 | REL-01 | docker compose -f infra/compose.production.yml config --quiet | pending, file not yet created |

## Wave 0 Requirements

- [ ] Проверить фактические package/test commands предыдущих фаз и создать отсутствующие test fixtures.
- [ ] Подтвердить изоляцию integration environment и local provider mocks.
- [ ] Измерить focused test runtime; установить feedback budget до execute.

## Sampling Rate

После каждого task commit — focused автоматическая проверка из task verify; после plan wave — integration/UI checks плана. Перед verifier — весь затронутый suite зелёный. Live команды не запускать как бессмысленный CI без credentials; отсутствие доступа остаётся Pending evidence.

## Manual-Only Verifications

- Руководитель проверяет подготовленные pilot/quality/load отчёты и UAT: правильность первичности, критериев оценки, правил перезвона и связи со сделкой. Подтверждение относится к конкретным проверенным результатам, а не обещанию качества. Production target и решение о публикации фиксируются отдельно, если ещё не заданы.

## Validation Sign-Off

- [ ] All tasks have automated check or explicit live/manual evidence
- [ ] Sampling continuity and Wave 0 measured
- [ ] Independent verifier and UAT completed
- [ ] Set nyquist_compliant true only after actual execution evidence
