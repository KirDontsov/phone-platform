---
phase: "02-platform-auth"
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-11-11
---

# Phase 2: Validation Strategy

Реализация отсутствует; команды целевые, не запускались. До execute актуализировать scaffold и измерить latency. Nyquist compliance не заявлена.

## Test Infrastructure

Rust cargo focused tests; отдельные PostgreSQL/RabbitMQ/storage в изолированном Compose project; Vue TypeScript/build/Vitest/Playwright. Rust tooling вводит 02-01, Vue commands/Playwright — 02-02, DB tests/full smoke — 02-03.

## Per-Task Verification Map

| Task ID | Requirement | Automated command | Status |
|---|---|---|---|
| 02-01-1 | PLAT-01 | cargo check --workspace; cargo test --workspace | pending, artifacts not yet created |
| 02-01-2 | PLAT-01 | docker build -f phone-api/Dockerfile -t phone-platform-api:scaffold .; docker build -f phone-worker/Dockerfile -t phone-platform-worker:scaffold . | pending, artifacts not yet created |
| 02-02-1 | PLAT-01 | npm --prefix phone-client run build; npm --prefix phone-client run type-check | pending, artifacts not yet created |
| 02-02-2 | PLAT-01 | docker build -f phone-client/Dockerfile -t phone-platform-client:scaffold .; npm --prefix phone-client run test:e2e -- --list --pass-with-no-tests | pending, artifacts not yet created |
| 02-03-1 | PLAT-01 | cargo test -p phone-api --test migrations; cargo test -p persistence; cargo check --workspace | pending, artifacts not yet created |
| 02-03-2 | PLAT-01 | docker compose -f infra/compose.yml -f infra/compose.test.yml config --quiet; scripts/smoke-compose.sh | pending, artifacts not yet created |
| 02-04-1 | AUTH-01, AUTH-02 | cargo test -p phone-api --test auth | pending, artifacts not yet created |
| 02-04-2 | AUTH-01, AUTH-02 | npm --prefix phone-client run type-check; npm --prefix phone-client run test:e2e -- auth | pending, artifacts not yet created |
| 02-05-1 | AUTH-02, AUTH-03 | cargo test -p phone-api --test tenant_isolation | pending, artifacts not yet created |
| 02-05-2 | AUTH-02, AUTH-03 | npm --prefix phone-client run type-check; npm --prefix phone-client test -- members; npm --prefix phone-client run test:e2e -- invite-accept | pending, artifacts not yet created |

## Wave 0 Requirements

- [ ] 02-01 Rust build/tests and image build
- [ ] 02-02 Vue build/type-check/test/test:e2e scripts и Playwright config
- [ ] 02-03 migration fixtures и отдельный Compose test project
- [ ] 02-04 backend auth и tests/e2e/auth.spec.ts; 02-05 tenant/invite acceptance E2E

## Sampling

После каждого task commit — automated focused checks из task verify; после wave — проверки плана, после 02-03 обязательно полный isolated smoke. Перед verifier весь затронутый suite зелёный. Config validation и test-list проверяют конфигурацию, не заменяют реально выполненный бизнес-сценарий.

## Manual Gates

Обязательная матрица двух компаний: отдельные owners/connections/settings, одинаковые provider IDs без склейки, user с разными ролями, membership-checked company selection и сброс UI при смене, поздний response/SSE предыдущей компании не применяется. API auth и browser auth suite покрывают эти случаи; job/media/connector checks дополняют матрицу в своих фазах.

UAT owner bootstrap→login→reset/logout и invite email→accept/setup→login с ролями. Всё автоматизируемое, включая тестовый email и получение ссылки из mail catcher, выполняет executor.

## Validation Sign-Off

- [ ] Focused/domain/integration checks passed
- [ ] Compose full smoke and cleanup passed
- [ ] Recipient E2E passed including expired/revoked/reused
- [ ] Independent verifier and UAT completed
