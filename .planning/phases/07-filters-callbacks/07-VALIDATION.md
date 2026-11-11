---
phase: "07-filters-callbacks"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-11-11
---

# Phase 7: Validation Strategy

Реализация отсутствует; команды целевые, не запущены. До execute актуализировать контракты/scaffold, измерить latency checks и завершить Wave 0. Nyquist compliance пока не заявлена.

## Test Infrastructure

Rust cargo focused/domain/integration tests в изолированных DB/broker/storage; Vue TypeScript/build/Vitest/Playwright по соответствующим UI действиям; внешние adapters по sanitized fixtures и отдельному live evidence.

## Per-Task Verification Map

| Task ID | Requirement | Automated command | Status |
|---|---|---|---|
| 07-01-1 | FLT-01, FLT-02 | cargo test -p phone-api --test filter_compiler | pending, file not yet created |
| 07-01-2 | FLT-01, FLT-02 | npm --prefix phone-client run build | pending, file not yet created |
| 07-02-1 | FLT-03, FLT-04 | cargo test -p phone-worker --test callback_rules | pending, file not yet created |
| 07-02-2 | FLT-03, FLT-04 | cargo test -p phone-worker --test callback_clock | pending, file not yet created |

## Wave 0 Requirements

- [ ] Проверить фактические package/test commands предыдущих фаз и создать отсутствующие test fixtures.
- [ ] Подтвердить изоляцию integration environment и local provider mocks.
- [ ] Измерить focused test runtime; установить feedback budget до execute.

## Sampling Rate

После каждого task commit — focused автоматическая проверка из task verify; после plan wave — integration/UI checks плана. Перед verifier — весь затронутый suite зелёный. Live команды не запускать как бессмысленный CI без credentials; отсутствие доступа остаётся Pending evidence.

## Manual-Only Verifications

UAT по реальному интерфейсу и проверка бизнес-правил. Live ASR/model/CRM/Sheets проверки при затронутом провайдере обязательны отдельно от fixtures; всё автоматизируемое выполняет executor.

## Validation Sign-Off

- [ ] All tasks have automated check or explicit live/manual evidence
- [ ] Sampling continuity and Wave 0 measured
- [ ] Independent verifier and UAT completed
- [ ] Set nyquist_compliant true only after actual execution evidence
