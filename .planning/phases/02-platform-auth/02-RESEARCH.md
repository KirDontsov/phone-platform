# Phase 2: research

**Date:** 2026-11-11
**Status:** baseline; MUST revalidate before execute

Baseline из research/BONDSCAN.md и ARCHITECTURE.md. Версии зависимостей, cookie/CSRF и SMTP/reset flow перепроверить перед execute; существующего кода в новом проекте пока нет.

## Sources

- [.planning/research/BONDSCAN.md](../../../.planning/research/BONDSCAN.md)
- [.planning/ARCHITECTURE.md](../../../.planning/ARCHITECTURE.md)

## Architectural Responsibility Map

API проверяет сессию, membership и команды; worker выполняет долговременные внешние задачи; PostgreSQL хранит durable state и версии; Vue выводит серверные состояния, а не выдумывает business truth.

## Constraints and Pitfalls

D-01: Rust Actix/Diesel/Tokio, Vue/Pinia/Tailwind. D-02: Monorepo и инфраструктура, независимая от Bondscan; общая PostgreSQL нового сервиса с изоляцией компаний по organization_id. D-03: Несколько компаний с первого запуска; membership и выбранный tenant проверяет сервер, данные/credentials/settings изолированы. Переключение компании не сохраняет чужие stores/cache и не принимает старые responses/SSE. D-04: Закрытые приглашения, без публичной регистрации.

Домен, SMTP/reset канал, storage backend и production host. Local Compose и mail catcher не требуют production секретов.

## Validation Architecture

- 02-01: API и worker собираются в workspace без отсутствующих shared crates. Образы приложений не содержат секретов и не подключаются к инфраструктуре Bondscan. Проверки в PLAN.md.
- 02-02: Vue SPA собирается через реальные HTML/Vite/TypeScript entrypoints. Общий API client и Playwright готовы для последующих auth/invite экранов. Проверки в PLAN.md.
- 02-03: На чистой БД повторяемо создаются tenant-aware access, jobs и outbox schema до production sync. Изолированный Compose build/up/migrate/readiness smoke доказывает полный запуск платформы. Проверки в PLAN.md.
- 02-04: Вход сохраняется после refresh, logout и reset отзывают серверные сессии. Браузер не может подменить компанию или выполнить изменение без CSRF-защиты. Проверки в PLAN.md.
- 02-05: Владелец приглашает сотрудников и сопоставляет добавочные, ограниченные роли не управляют подключениями. Два tenant с одинаковыми object IDs изолированы на HTTP и job/media policy. Проверки в PLAN.md.

Shared foundation 02-03 создаёт jobs/outbox до phase3. Compose smoke включает build/up --wait/migration/API dependency readiness/worker dependencies и изолированный cleanup; config validation сам по себе не закрывает PLAT-01. Generic API client создан 02-02; auth использует его без дополнительного shared-client изменения. Recipient invite acceptance проверяется отдельным Playwright E2E в 02-05. Все планы менее 15 явно перечисленных файлов, lockfiles scaffold включены.

Sampling contract — 02-VALIDATION.md, status draft; до execute измерить test latency и перепроверить library versions.
