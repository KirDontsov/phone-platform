# Phase 2: Основа, авторизация и компании

Status: planned, implementation not started. Updated: 2026-11-11.

## Goal

Поднять независимую платформу с безопасными сессиями, компаниями и сотрудниками.

## Decisions

D-01: Rust Actix/Diesel/Tokio, Vue/Pinia/Tailwind.

D-02: Monorepo и независимые БД/очереди.

D-03: Tenant membership — серверный источник прав.

D-04: Закрытые приглашения, без публичной регистрации.

## Dependencies

Перед execute завершить фазу 1 и прочитать её SUMMARY/VERIFICATION; смена контрактов требует replan.

## Execution Sequence

- 02-01: Rust API/worker, images и .env.example; 9 изменяемых путей, Cargo.lock включён.
- 02-02: Vue entrypoints/build/API client/Playwright; 10 путей, package-lock включён.
- 02-03: Shared crates, access/jobs/outbox schema, Compose и полный migration/readiness smoke; 14 путей.
- 02-04: Session auth, CSRF, reset; 14 путей.
- 02-05: Roles/invites/recipient acceptance E2E; 12 путей.

Waves 1→2→3→4→5 и явные depends_on. PLAT-01 закрывается после 02-03, jobs/outbox schema готова до production sync фазы 3. Никаких path dependencies на несуществующие crates и обещания migration-ready на scaffold этапах.

## Open Questions

Домен, SMTP/reset канал, storage backend и production host. Local Compose и mail catcher не требуют production секретов.

## Source Coverage

| Source | Coverage |
|---|---|
| GOAL | 02-01 Rust; 02-02 Vue; 02-03 foundation/Compose; 02-04 auth; 02-05 invites/roles |
| REQ | PLAT-01, AUTH-01, AUTH-02, AUTH-03; распределение в frontmatter планов |
| RESEARCH | Каждый constraint в RESEARCH.md должен быть актуализирован и проверен указанными validation сценариями |
| CONTEXT | D-01…D-04 включены в действия планов; явные исключения не добавлять без изменения контекста |

## Deferred Ideas

Публичный биллинг, новая АТС и речевой бот не входят в эту фазу. Лидирование не подтверждено и не выводится из слова «легирование».
