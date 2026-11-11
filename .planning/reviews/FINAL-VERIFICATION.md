# Сквозная проверка реализации и готовность к финальному UAT

Дата: 2026-11-11. Проверены все **38 v1 requirements**, фазовые execution summaries/verification, реализационный review, контракты и фактические локальные evidence. Рекомендуемый статус поставки: **ready_for_final_uat**. Это готовность локального сервиса к пользовательскому тесту; это не закрытие M0, не production release и не подтверждение качества настоящих разговоров.

Независимость: verifier не реализовывал root API, shared crates, frontend, backup/eval tooling. Worker реализован этим же исполнителем; его внутреннее review помечено self-review в [IMPLEMENTATION-REVIEW](IMPLEMENTATION-REVIEW.md). Часть интеграционных запусков выполнена другими исполнителями; ниже это явно отделено от собственных команд проверяющего. GSD PLAN-CHECK проверяет качество планов, не заменяет этот документ.

## Фактическое evidence

| Область | Результат и provenance | Чего результат не доказывает |
| --- | --- | --- |
| Rust | Проверяющий выполнил `cargo check --workspace`, workspace43 PASS, отдельно API real-PG2 и persistence real-PG1; после единственного shared-media purge fix — workspace compile и targeted pipeline4/4. Совокупно47 различных tests: worker29/domain13/contracts2/API2/persistence1. После access-audit fix targeted API2/2 повторён с новыми assertions | Не 47 новых тестов на каждый повтор; default ignored/skip не засчитываются. Не проверяет реальные аккаунты |
| Телфин CLI | Предыдущий выполненный CLI review slice:56 offline PASS, включая enum/resume/параметры/границы/провалы; настоящие FS/SQLite, synthetic transport | Не доказывает права/полноту конкретной АТС |
| Frontend | [FRONTEND-VERIFICATION](FRONTEND-VERIFICATION.md):20 unit PASS, production build PASS, финальные Docker nginx E2E13/13 без skips44.1s. Screenshots QA JSON: viewport1440/390, overflow0, page errors0, audio duration32/ready4/errornull | Это synthetic demo, не распознанный разговор; один сценарий отказа switch преднамеренно подменяет503 |
| Clean Compose | Прочитан `data/eval/compose-smoke.json`: passed,5 checks, отдельные чистые volumes, настоящие API/worker/DB/client/media/queue, company-switch чужой call404, live requests0; временный project очищен | Не внешнее развёртывание и не бесконечная масштабируемость |
| Worker packaging/health | Root исправил ошибку COPY каталога вместо бинарника: staging `/tmp/phone-worker`. Проверяющий независимо выполнил `docker exec phone-platform-worker-1 phone-worker healthcheck` — exit0; текущий контейнер healthy | Не model inference и не provider acceptance |
| OpenCode | Прочитан `data/eval/model-runner-smoke.json`: реальный Docker/HTTP1.18.10, nonroot/readonly/tmpfs/caps/security limits, anonymous rejected, MCP0/plugins0, session lifecycle, model requests0. Worker HTTP fixtures проверяют deny-all всех инструментов | Доступность Codex/выбранной модели и качество не проверялись |
| Query load | Прочитан `data/eval/load-report.json`:5000 synthetic calls,4 parallel,32 measured requests, p95 endpoints517–752ms, counts/filter/callback/tenant/pagination/null-score PASS; tenants удалены | Это небольшой local query smoke; не audio/backfill throughput, SLA или фактический профиль компании |
| Backup/restore | Прочитан `data/eval/backup-verify.json`: isolated_restore_verified=true; snapshot2org/4users/241docs/9jobs,22ready media, ledger1,5post-backup docs removed, shared media retained1, no_resurrection=true, sessions revoked, original DB не заменена. Root выполнил настоящий create→isolated restore | Нужны отдельно сохранённые ключ и свежий deletion ledger. Production cutover/RPO/RTO и disaster потеря всех локальных носителей не проверены |
| Offline quality harness | Реализованы `eval-transcripts.py`/`eval-analysis.py`, synthetic tests и no-reference CLI smoke. Оба прочитанных отчёта имеют reference_samples0 и acceptance_passed=null | Harness не выдаёт отсутствие корпуса за хорошее ASR/AI качество. Нет живой quality acceptance |

Последние результаты вспомогательных Python tests и Markdown links перечислены в [EXECUTION-DELTA](../research/EXECUTION-DELTA.md) и [pilot-report](../../docs/pilot-report.md). Этот verifier не пересчитывает повторы как новые tests. Машинные отчёты хранятся локально в `data/eval`, не содержат реальных клиентских аудио/секретов.

## Реестр 38 requirements

Обозначения:

- **L** — указанное поведение реализовано и проверено локально на настоящем стеке/синтетических данных. Пользовательский UAT остаётся открытым.
- **E** — реализация/контракт локально проверены в указанном объёме; внешняя часть требования ещё не подтверждена.
- **D** — явно принятое техническое отклонение от исходного плана/часть requirement не реализована; это не external PASS.
- **P** — конкретный локальный пробел требует исправления либо явно остаётся незакрытым.

| Requirement | Статус | Реализованное evidence / оставшаяся проверка |
| --- | --- | --- |
| TEL-01 | E | CLI credential/scope/error guards56tests; реальный OAuth и права конкретной АТС pending |
| TEL-02 | E | CLI pages/windows/checkpoint, whole-call enums/no-recording сохранены; вся доступная история конкретной АТС не сверена |
| TEL-03 | E | Durable report/reconciliation/coverage реализованы; контрольные totals с Телфином и нижняя граница pending |
| SYNC-01 | E | Rust import/checkpoint/idempotency/raw snapshots/guards; реальный initial backfill и restart/fault-injection провайдера pending |
| SYNC-02 | E+D | Polling5min/2day overlap/manual reconciliation; webhooks/subscriptions/inbox отсутствуют, поздние изменения вне overlap требуют повторного периода |
| SYNC-03 | E | Call/CDR раздельно, IVR/answered elsewhere semantics и callback tests; реальные переводы/плечи конкретной АТС pending |
| SYNC-04 | E | Lease/fencing/outbox/retry/failed/uncertain, OAuth/429 guards; Rust live expiry/quota/restart acceptance pending. Dead jobs в БД, не отдельная Rabbit DLQ |
| PLAT-01 | L | Финальные Docker images/health, clean-volume Compose5checks, независимый healthcheck, повторяемые миграции |
| AUTH-01 | L | Реальные HTTP/PG/Mailpit/browser login/logout/reset one-use/session reload; SMTP/TLS сервера остаются deployment config |
| AUTH-02 | L | Tenant-scoped sessions/membership/roles, composite FK/jobs/media, две компании в HTTP/E2E/clean smoke |
| AUTH-03 | L | Invite/roles/revocation/extension CRUD, explicit CDR/config ID→number map, membership attribution PG test с чужой компанией; автосправочника Телфина нет |
| CONN-01 | E | Owner UI/API/save/redaction/test/disconnect/sync/period/history-start; настоящий Telphin connection test не запускался |
| CALL-01 | L | Журнал/периоды/search/cursor/данные/стадии через реальный API/browser; все типы звонка остаются независимо от ASR |
| CALL-02 | L | Карточка/CDR/history/note/empty/errors/versions; desktop/mobile screenshots/E2E |
| CALL-03 | L | Настоящий authorized Range/playback на Docker media volume; synthetic WAV32sec, foreign media guards. Реальные codec/storage expiry pending |
| TAG-01 | L | CRUD/archive tag/status, отдельный provider_result; archived retain/new-assignment forbidden HTTP/UI regressions |
| TAG-02 | L | Автор/optimistic409/manual_lock, PG reprocess/import preservation и browser annotation/history |
| ASR-01 | E | Multipart ASR transport loopback, timestamps/unknown roles/версии/role confirmation; реальная русская ASR/диаризация pending |
| ASR-02 | E | Bound downloads/magic/ffprobe, chunking600s/temp cleanup/timeouts/reprocess/нет конфигурации; настоящий long/noisy corpus pending |
| AI-01 | E | Реальный изолированный OpenCode metadata/session smoke, generic OpenAI/http adapters и strict JSON loopback; inference/Codex login/модель pending |
| AI-02 | L | Структура summary/intent/agreements/next steps/taxonomy/criteria/evidence валидируется, quote/time tied to transcript; смысловая корректность требует референсов |
| AI-03 | L | Immutable rubric_version/weights/required/score/null/abstention, settings/card/version UI; реальные критерии согласует UAT |
| AI-04 | L | Primary/repeat/unknown вычисляются по более ранней истории/coverage, IVR не считается разговором; ручной override сохраняется. Бизнес-смысл меток ждёт UAT |
| AI-05 | L | Model/prompt/rubric/transcript/hash/history, schema fail-closed, fencing-based daily reservations и budget retry regression; это estimate, не подтверждённый billing |
| FLT-01 | L | Совместимые period/direction/employee/result/taxonomy/score/stage фильтры в API/list/dashboard/load |
| FLT-02 | L+D | Bounded typed AST AND/OR/NOT/saved views, evaluator общий; SQL compiler/read models из плана заменены in-memory evaluator |
| FLT-03 | L | Callback episode before display filter, attempt/success/coverage/work-hours/threshold, dashboard→same selection; IVR regression |
| FLT-04 | L | Свежий as_of на каждом запросе/UI polling, clock-only overdue/DST/late-history recompute tests; отдельной callback cache/job таблицы нет |
| CRM-01 | E | OAuth/link/search/ambiguity UI и безопасные adapters реализованы; реальный account/scopes/refresh/revocation/доступ к сделке pending |
| CRM-02 | E | Stable marker/pinned version/per-call lock/uncertain reconciliation без blind POST; фактическая заметка/идемпотентность в amoCRM pending |
| SHEET-01 | E | Column mapping/stable key A/RAW/upsert/duplicates/unknown commit protections; OAuth/quota/реальный лист pending; concurrent manual sort неатомарен |
| INT-01 | E | Независимые jobs/retries/deliveries/errors/disconnect/reconciliation; настоящий provider revoke/quota acceptance pending |
| DASH-01 | L | Реальный dashboard/list/filter consistency, raw call result и human connection разделены, synthetic counts/load/browser |
| DASH-02 | L | Daily/employee/score buckets/nullable average, deterministic denominators и load consistency; производительность реального объёма не подтверждена |
| OPS-01 | L | Правила/подключения/разметка/access audit; финальный gap invite/member-role/removal/revoke исправлен root, targeted API real-PG2/2 PASS, tenant/actor/subject и forbidden-no-audit проверены |
| OPS-02 | L | Operations/jobs/errors/retry (включая uncertain)/reprocess/history/health, nullable costs и usage; external alerting отдельно оператором |
| OPS-03 | L | Retention/purge/tombstone/private media + настоящий snapshot→isolated restore и актуальный ledger; единый срок компании, operator at-rest encryption/backup schedule |
| REL-01 | E | Полный локальный synthetic Docker/UI/worker/load/recovery путь пройден; реальный Телфин→ASR→LLM→CRM pilot, согласование качества/нагрузки и пользовательский UAT pending |

Реестр содержит ровно 38 уникальных IDs без пропусков: 22 имеют локально подтверждённое поведение, 16 сохраняют external acceptance. Два пункта дополнительно помечены явным scope deviation; локальных открытых findings после описанного исправления — 0. Пользовательский UAT остаётся открытым для всей поставки.

## Конкретные замечания и ограничения сдачи

**OPS-01 / P2 — доступ без audit, устранено.** Во время сквозного review auth.rs endpoints `invite`, `update_member`, `remove_member`, `revoke_invite` не создавали audit documents. Root добавил атомарные invitation.created/revoked и membership.updated/removed/joined без email/token. Проверяющий добавил focused HTTP/PG regressions и выполнил targeted API suite:2/2 PASS (3.11s). Проверены tenant/actor/subject/previous-role, сохранение extension, отсутствие audit при forbidden/missing/foreign операции и безопасные revoke/remove events. Source API исправлен root; полный workspace/E2E не повторяли ради этой локальной правки, Docker targeted invitation regression выполняется root отдельно.

Незакрытых конкретных локальных P0/P1/P2 дефектов в прочитанном scope после этого исправления не осталось. Это не утверждение об отсутствии всех возможных дефектов и не pentest. Ранее найденные OAuth epoch, retention_days, optimistic imports, coverage continuity, budget retry, media orphan/shared path, uncertain retry и Docker artifact path исправлены и имеют указанное в [implementation review](IMPLEMENTATION-REVIEW.md) evidence.

Нельзя закрывать по этому локальному результату:

1. Полноту реальной истории и проверенную нижнюю границу Телфина. `all_available` без подтверждённого history_start остаётся bounded364day probe с coverage=false.
2. Качество настоящей русской речи/ролей/AI/evidence и стоимость модели. У harness reference_samples=0, acceptance=null; вымышленные demo transcripts не подходят для quality gate.
3. Настоящие OAuth scopes/token rotation/CRM/Sheets delivery и внешние quotas. Повторное uncertain reconciliation безопасно ищет результат и не даёт слепого POST/append.
4. Production deployment/SMTP/TLS/alerting/backup schedule, SLA/RPO/RTO и нагрузку audio/backfill pipeline. Локальная query capacity не универсальная гарантия.
5. Финальную оценку удобства/бизнес-правил пользователем. [UAT](../../docs/uat.md) готов, результат от пользователя не получен.

Финальная передача должна содержать локальный адрес, demo mode и ссылку на UAT/ограничения, а также явно перечисленные реальные подключения, которые пока не проверены. Чекбоксы требований и completed phases не должны превращать эти pending gates в PASS по факту наличия26summary/10verification.

## Финальное закрытие затронутых Docker-сценариев

На финальных audit API и nginx с динамическим DNS повторены 2 затронутых E2E: **2/2 PASS, zero skips, 14.8 s**. Через authenticated owner API подтверждена корреляция свежих `invitation.created` и `membership.joined` по invitation ID, без email/token в stdout. Отдельный реальный Docker backend IP swap `172.x.2 → 172.x.4` сохранил frontend container ID и полный URI/query: PASS. Это повтор двух сценариев из полного набора 13, а не увеличение числа различных E2E до 15. Source commits: `e789168` (платформа), `7af83b7` (эксплуатация), `3563c4f` (access audit/DNS). Source freeze полный; эта последняя запись меняет только документацию.
