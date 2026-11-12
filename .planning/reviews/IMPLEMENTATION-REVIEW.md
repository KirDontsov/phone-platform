# Финальное implementation review

Дата: 2026-11-11. Объект: реализованные Rust API/shared crates/worker и их HTTP/DB-контракты. Результат: **автоматические проверки пройдены; внешняя приёмка и пользовательский UAT остаются Pending**. Это проверка реализации, а не перенос planning PASS на продукт.

Проверяющий — отдельный исполнитель относительно root API/shared crates; worker был реализован этим же исполнителем, поэтому его часть является дополнительным self-review, а не независимым аудитом. Проверка frontend в браузере, Docker lifecycle/restore и качество планов имеют отдельные evidence; этот документ не заявляет их собственным выполнением.

## Выполненные проверки

| Проверка | Фактический результат |
|---|---|
| `cargo check --workspace` | PASS, итоговые shared/API/worker компилируются без warning |
| `cargo test --workspace -- --test-threads=1 --nocapture` с изолированной `TEST_DATABASE_URL` | 43 PASS; 3 явно ignored DB-теста выполнены отдельно ниже |
| `cargo test -p phone-api --test platform -- --ignored --test-threads=1 --nocapture` | 2 PASS на отдельной phone_platform_api_test |
| `cargo test -p persistence --test postgres -- --ignored --test-threads=1 --nocapture` | 1 PASS на отдельной phone_platform_domain_test |
| После дополнительного shared-audio purge fix: `cargo check --workspace`; `cargo test -p phone-worker --test pipeline -- --test-threads=1 --nocapture` | PASS; targeted pipeline 4/4, включая новый shared-audio regression; workspace check без warning |
| CLI unittest suite предыдущего CLI review slice | 56 PASS; новые worker fixes CLI не меняют |

Секреты локального `.env.platform` использовались только программно для адресации изолированных dev DB; значения не печатались. Живые аккаунты Телфина/ASR/LLM/CRM/Google не вызывались. Loopback HTTP fixtures используют синтетический payload и отдельный allowlist только внутри тестов.

Совокупно проверено 47 различных Rust tests: worker 29, domain 13, contracts 2, API 2, persistence 1. Полный workspace не повторяли после единственного дополнительного purge fix; выполнены затронутый pipeline suite и workspace compile.

Worker baseline в workspace: 6 library tests (один с настоящим PostgreSQL), 14 provider/guard tests, 5 loopback HTTP tests, 3 pipeline PostgreSQL tests; дополнительный shared-audio regression увеличивает pipeline до 4. Наличие SKIP при отсутствии TEST_DATABASE_URL не считается DB evidence: перечисленные прогоны выполнялись с реальными отдельными PostgreSQL БД.

## Обнаруженные и устранённые проблемы

1. **P1 — OAuth expiry type mismatch.** API перезаписывал epoch `expires_at` строкой RFC3339; worker ожидал integer и не запускал refresh. Root сохранил epoch transport-контракта и epoch fallback. Реальный OAuth flow пока не проверен: исправление подтверждено кодом/типом контракта, не внешней авторизацией.
2. **P1 — manual retention запускал 90 дней вместо настройки компании.** Root передаёт актуальный org.retention_days в payload. HTTP/DB regression меняет срок на 365 дней и проверяет созданную задачу.
3. **P1 — импорт мог перетирать параллельные ручные поля.** `import_call` сохраняет существующий звонок с optimistic version; same-call imports сериализуются transaction advisory lock. Конфликт даёт безопасный retry. Остальные изменения анализов/стадий уже используют latest call + expected_version. PG regression подтверждает сохранение ручной заметки/lock/классификации и coverage при повторном импорте; исчерпывающий adversarial stress всех interleaving не проводился.
4. **P1 — новые polling-звонки теряли подтверждённое coverage.** На connection теперь сохраняется verified interval, успешный перекрывающийся polling продлевает его; gaps остаются unknown. Coverage goal сохраняется в checkpoint и не пересчитывается из изменённой настройки при resume. Regression проверяет overlap, gap, непроверенную и изменившуюся нижнюю границу.
5. **P1 — ручной retry обходил daily budget.** Attempts сбрасывается при retry, поэтому reservation по attempts переиспользовалась. Reservation теперь основана на монотонном fencing. PG regression сначала резервирует 0.75 из budget 1, вручную повторяет задание и получает `analysis_daily_budget_reached`; запрещённый model URL отвергается локально, платного запроса нет.
6. **P1 — отказ финальной транзакции оставлял скачанный artifact.** Имя media включает job/fencing, uncommitted artifact удаляется при отказе, старый worker не удаляет файл новой попытки. Retention дополнительно удаляет непривязанные tenant-файлы старше 24 часов после crash между fsync и DB commit. MIME без audio magic отвергается; ffprobe duration обязателен. Tests проверяют cleanup, magic и tenant boundary.
7. **P1 — shared demo recording удалялась при purge одного звонка.** Перед unlink учитываются другие tenant-local recording references того же path. Дополнительный PG regression проверяет сохранение файла после первого purge и удаление после последнего.
8. **P2 — добавочные не участвовали в attribution.** Worker сохраняет extension_id/number из CDR, использует явный проверенный `extension_mapping:{id:number}` при отсутствии номера, сопоставляет membership только своей компании. ID не угадывается как номер; неоднозначность показана явно. PG test с двумя компаниями и одинаковым добавочным подтверждает отсутствие чужого сотрудника.
9. **P2 — archived taxonomy и note-only annotation.** Root разрешил сохранять уже назначенный archived status/tag, запрещает новые назначения, фиксирует derived contact_kind перед manual_lock. Добавлены HTTP regressions. Версия rubric выделена в rubric_version; root сериализует бизнес-версию отдельно от optimistic row_version.
10. **P2 — uncertain delivery нельзя было сверить повторно.** Persistence разрешает tenant-scoped retry uncertain с новым fencing; worker при uncertain ищет marker/key, не выполняет слепой POST/append. Shared PG test проверяет state/marker/payload/fencing. Кнопка клиента корректируется владельцем frontend отдельно.
11. **P2 — AI taxonomy не совпадала с manual API.** Domain ограничивает suggested tags 30 уникальными допустимыми IDs; regression проверяет duplicates/limit. Rubric/quote/timestamp/score validation остаётся fail closed.
12. **P2 — provider fail-closed детали.** OpenCode config с plugins, MCP или разрешающими permission overrides отвергается; все discovered tool IDs выключаются в каждой новой session. Сессия удаляется, cleanup failure видим. Sheets mapping валидируется и пишет RAW; duplicate stable key запрещён. Явная 429 сохраняет retry, network/5xx после записи становится uncertain. Экспорт одного call/provider сериализуется, analysis/marker закреплены на delivery.

13. **P1 — Docker worker artifact path (runtime verification).** Root обнаружил, что `/app/phone-worker` уже являлся каталогом source COPY: build `cp` положил бинарник внутрь каталога, runtime COPY создал каталог вместо executable. Root изменил staging binary на `/tmp/phone-worker` и исправил runtime COPY. Cached image rebuild и полный Compose smoke прошли. Дополнительно проверяющий независимо выполнил `docker exec phone-platform-worker-1 phone-worker healthcheck`: exit0; контейнер healthy. Rust source не менялся, Cargo tests для packaging-only исправления не повторялись.

14. **P2 — аудит доступа (сквозной verifier).** Финальный cross-phase review обнаружил отсутствие audit в auth invite/member/revoke mutations. Root добавил атомарные invitation.created/revoked и membership.updated/removed/joined. Проверяющий расширил `phone-api/tests/platform.rs`: actor/tenant/subject и previous role, forbidden/missing/foreign no-audit, revoke/remove без email/token. Targeted real-PG API2/2 PASS после fix; полный workspace не повторяли. Итоговый реестр: [FINAL-VERIFICATION](FINAL-VERIFICATION.md).

## Проверенные архитектурные границы

- Сессия выбирает компанию через проверенное membership; HTTP tenant header проверяется против сессии. Origin/CSRF обязателен для authenticated writes. RBAC и чужие call/tag/media/query операции покрыты API PG regression.
- SQL пользовательских значений параметризован; родительские документы и jobs имеют составные tenant FK. Jobs/outbox создаются атомарно; stale fencing/lease не завершает задачу. Финальные worker documents и job success сохраняются в одной транзакции. Deletion tombstones запрещают позднее воскрешение.
- Whole-call enums отделены от CDR. IVR `answered` не закрывает callback: требуется bridged/подтверждённый human_connected и порог разговора. Domain tests проверяют ответ IVR, попытку, успешный перезвон, clock-only overdue, неполное coverage, рабочее время и DST. API test дополнительно подтверждает, что фильтр списка не скрывает разрешающий звонок из истории callback.
- Импорт всех метаданных не зависит от записи/ASR. Sync page/checkpoint фиксируются атомарно; 401/429, owner, window, page/order/per_page, repeated page и bounded payload проверяются. История без verified lower bound не объявляется полной.
- ASR — отдельный multipart adapter, точные роли не выводятся из Speaker A/B. Loopback test проверяет URL `/v1/audio/transcriptions`, multipart file/model/format и unknown roles. Анализ без известных ролей/доказательств остаётся abstention/null, а не нулевой оценкой.
- AI JSON не ремонтируется; evidence цитирует реальные segments и допустимые времена, итог взвешивается trusted rubric. История transcript/analysis сохраняется; manual_lock не перетирается. Synthetic demo требует одновременно явного DEMO_MODE и synthetic call.
- Записи приватны, tenant path проверен, произвольные provider storage URL и редиректы не используются. В обычные логи не попадают токены, аудио и номера клиентов. Nginx access format исключает query string.

## Открытые ограничения и release gates

- **M0/live Pending.** Без verified history_start режим all_available выполняет ограниченный 364-day probe и оставляет coverage incomplete. Это не доказанная «вся доступная история». Для конкретной АТС ещё нужны реальные полнота/доступы/граница/лимиты/поздние записи и контрольные totals. Resume worker покрыт code review/checkpoint guards; провайдерный end-to-end сбой страницы подтверждён CLI fixtures, но не реальным Telephin/Rust HTTP fault-injection стендом.
- Webhook subscriptions/inbox не реализованы; актуальность обеспечивают polling каждые пять минут и ручная сверка периода. Поздняя запись за пределами двухдневного overlap требует повторного импорта. Это отклонение от первоначального плана phase 3, не скрытая готовая возможность.
- ASR chunking ffmpeg (600 s, PCM mono16k, bounded files/timeout/temp cleanup) реализован; реальная длинная русская запись, channel-role validation и held-out ASR/LLM quality не проверялись. Synthetic demo не доказывает точность.
- OpenCode transport/session/tool deny проверены fixtures; Docker runner smoke без model prompt предоставлен отдельным domain executor. Доступ к конкретной модели/Codex и качество настоящего анализа остаются live gate.
- OAuth tokens/scopes, amoCRM deal/contact ownership, Sheets quotas и реальные ambiguous commit/reconciliation требуют отдельных тестовых аккаунтов. Sheets read-then-write не атомарен относительно одновременной ручной сортировки; во время доставки не редактируйте управляемый диапазон. Unresolved uncertain требует внешней сверки; повторный blind append/POST отсутствует.
- Employee mapping не загружает справочник добавочных автоматически; нужны явный номер в CDR либо проверенный ID→number config. После изменения сопоставления повторяется синхронизация периода.
- Daily budget — консервативная reservation, не биллинг. Неизвестная стоимость сохраняется null; API отдельно показывает известную стоимость/число неизвестных. Model endpoint context/output bounded, реальные тарифы и нагрузка не подтверждены.
- Calls/filter/dashboard сейчас загружают tenant history и вычисляют AST/episodes в памяти. SQL read models/потоковая история нужны при существенно большей нагрузке; результаты отдельного локального synthetic load не доказывают production SLA. Точный объём пользователя ещё не задан.
- Документ не подтверждает production deployment, восстановление backup, visual UAT или полноту всех 38 requirements только по наличию файлов. Их фактические отчёты и конечный пользовательский тест учитываются отдельно.

## Повторное review — 2026-11-12

Root проверил текущие worker/API/frontend контракты и внёс maintenance fixes. Это собственное review изменений, не новый независимый аудит.

- **P1, смысловая разметка:** LLM получала только UUID разрешённых тегов/статусов без названий. Теперь вход дополнен tenant-scoped `tag_definitions` / `status_definitions` с ID и названием, архивные значения исключены. UUID allowlists и строгая серверная валидация сохранены. Unit regression + настоящий loopback OpenAI-compatible transport подтверждают передачу названий; реальная точность выбора требует live quality evaluation.
- **P1, неопределённая доставка:** ошибка чтения/обновления credentials во время reconciliation переводила delivery из uncertain в failed. Следующий retry мог снова разрешить POST/append. Теперь `requires_reconciliation` сохраняется до подтверждённой доставки, в том числе после повторной ошибки и при прежнем attempting после crash. Подготовка связи со сделкой/колонок выполняется до отметки attempting. Unit проверяет цепочки timeout/429/permission/configuration errors; реальная отдельная PostgreSQL проверяет сохранение uncertain/marker после configuration failure и ручного retry. Внешние CRM/Sheets не вызывались.
- **P2, отключение:** явный disconnected ASR/analysis игнорировался и в DEMO_MODE подменялся учебным провайдером. Теперь явное отключение даёт configuration_missing; fallback допустим только при отсутствии документа подключения. PG regression подтверждает отсутствие выдуманной расшифровки после отключения.

Свежие проверки: worker33/33 с TEST_DATABASE_URL отдельной phone_platform_review_test (8 library, 5 loopback HTTP, 6 PG pipeline, 14 provider), contracts/domain15/15, Python80/80, frontend20/20; git diff --check PASS. 4 новых Rust regressions увеличивают совокупный ранее проверенный набор с47 до51; API2/persistence1 отдельно повторно не запускались, полный browser13 здесь не повторялся. Docker API/worker пересобраны; актуальный целевой smoke записан в data/eval/review-20261112-smoke.json.

Для пользователя добавлена короткая инструкция в docs/uat.md. Live acceptance Телфин/ASR/LLM/CRM/Google остаётся открытым.
