# Phase17 — независимая проверка плана

Дата: 2026-11-12. Проверяющий: timestamp_plan_checker; автор плана — root.

**Результат: PASS, revision-loop 0/3.** Это проверка плана, не подтверждение реализации или production.

- Причина соответствует коду: `Some("")` из необязательной `history_start` попадает в строгий parser до сохранения sync_run. Диагностика реального API описана автором research; проверяющий самостоятельно production/секреты не читал.
- Исправление ограничено выбором диапазона импорта. Отсутствующие/пустые/пробельные optional input dates используют прежние defaults; неверные непустые даты, timestamps CDR и сохранённые checkpoint dates остаются ошибками. Схема БД, авторизация, данные и конфигурация компаний не изменяются.
- Регрессии вызывают helper, используемый Worker::sync: all_available+blank history_start, blank from/to, whitespace, явный диапазон, приоритет и строгость checkpoint, невалидный/слишком большой диапазон. Проверить RED без фильтрации и GREEN после; неизвестная нижняя граница не становится verified coverage.
- Уточнение исполнения retry: `persistence::retry_job` обновляет только существующий jobs row; Worker::run регулярно вызывает claim_job напрямую из PostgreSQL. Для этого же задания не нужен новый outbox event и нельзя создавать дубль. Достаточен один атомарный UPDATE с guards org/job/subject/kind/state/error и сохранением существующей семантики fencing/lease/attempts, с возвращённым receipt. Исходное требование «outboxatomic» означает отсутствие разрыва состояния при повторной постановке; обязательной вставки в outbox нет.
- Выпуск использует существующий source-only native build и замену release tag без переноса/очистки БД и секретов. Возобновлять только найденное прерванное задание после нового healthy worker. Подтвердить настоящие checkpoint/import progress и исчезновение прежней ошибки; полную доступную историю или её нижнюю границу по первой странице не объявлять.
- Независимый review/verifier, состояние старого owner/регистрации/Codex/tunnel, неизменность phase12 stash, GitHub HEAD и финальный user UAT предусмотрены. Live ASR, качество моделей и соседние приложения вне исправления.

GSD-tools verify plan-structure: valid=true, errors/warnings=[]; две задачи с files/action/verify/done. Блокирующих замечаний нет.
