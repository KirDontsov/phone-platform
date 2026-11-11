---
phase: "06-ai-analysis"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-11-11
---

# Phase 6: Validation Strategy

Реализация отсутствует; команды целевые, не запущены. До execute актуализировать контракты/scaffold, измерить latency checks и завершить Wave 0. Nyquist compliance пока не заявлена.

## Test Infrastructure

Rust cargo focused/domain/integration tests в изолированных DB/broker/storage; Vue TypeScript/build/Vitest/Playwright по соответствующим UI действиям; внешние adapters по sanitized fixtures и отдельному live evidence.

## Per-Task Verification Map

| Task ID | Requirement | Automated command | Status |
|---|---|---|---|
| 06-01-1 | AI-01, AI-05 | python3 scripts/smoke-model-runner.py --fixture scripts/fixtures/model-transcript.json --report data/eval/model-smoke.json | pending, file not yet created |
| 06-01-2 | AI-01, AI-05 | cargo test -p phone-worker --test runner_isolation | pending, file not yet created |
| 06-02-1 | AI-02, AI-03, AI-05 | cargo test -p phone-worker --test analysis_validation | pending, file not yet created |
| 06-02-2 | AI-02, AI-03, AI-05 | cargo test -p phone-api --test rubrics | pending, file not yet created |
| 06-03-1 | AI-02, AI-03, AI-04, AI-05 | cargo test -p phone-worker --test classification | pending, file not yet created |
| 06-03-2 | AI-02, AI-03, AI-04, AI-05 | npm --prefix phone-client run build | pending, file not yet created |

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
