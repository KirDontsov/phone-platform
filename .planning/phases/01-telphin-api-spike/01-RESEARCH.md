# Phase 1: research

**Date:** 2026-11-11
**Status:** documentation-researched; live-unverified

Документация исследована 2026-11-11; live-проверки не выполнены. Источник — research/TELPHIN.md: trusted OAuth client_credentials, call_history с CDR, gzip, почасовые RateLimit headers. per_page, нижняя граница истории и границы времени требуют реальной проверки.

## Sources

- [.planning/research/TELPHIN.md](../../../.planning/research/TELPHIN.md)

## Architectural Responsibility Map

API проверяет сессию, membership и команды; worker выполняет долговременные внешние задачи; PostgreSQL хранит durable state и версии; Vue выводит серверные состояния, а не выдумывает business truth.

## Constraints and Pitfalls

D-01: Импортировать историю вызовов, а не список записей. D-02: Не исключать missed/internal/no-recording. D-03: Фиксировать фактический scope и coverage. D-04: Spike на Python stdlib, production на Rust.

Права клиента, hostname, доступная глубина истории, pagination termination, устойчивый CDR key и канал аудио. Пользователь подтвердил, что доступ к API и записям уже есть. Настроить его локально для выбранной компании, проверить права и принадлежность АТС; до реального probe/сверки M0 не закрывать.

## Validation Architecture

- 01-01: CLI выгружает все страницы выбранного периода независимо от записи и результата звонка. Недостаточные права, ошибка страницы или неизвестная нижняя граница истории видны пользователю. Сценарии и команды — в PLAN.md.
- 01-02: Есть воспроизводимый отчёт сравнения API и кабинета на контрольном периоде. Ложный missed из отдельного плеча не считается пропущенным клиентским звонком. Сценарии и команды — в PLAN.md.

Детальный sampling contract — 01-VALIDATION.md. Это стратегия будущей проверки, а не выполненные тесты. Сначала contract/fixture и component checks, затем meaningful integration/failure checks и UAT. Документационные fixtures не заменяют live API/model/pilot evidence.
