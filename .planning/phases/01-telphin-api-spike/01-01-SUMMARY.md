---
phase: "01-telphin-api-spike"
plan: "01"
subsystem: telphin-cli
status: completed_offline
requirements-completed: []
tags: [python, telphin, oauth, checkpoint, isolation]
key-files:
  created: [scripts/telphin-spike.py, scripts/tests/test_telphin_spike.py, docs/telphin-spike.md, docs/provider-contract.md, .env.example]
metrics:
  tests: 50
  latest_test_seconds: 0.158
  live_requests: 0
---

# 01-01 Summary — CLI готов и проверен offline

## Result

Stdlib CLI поддерживает `probe`, `export --from/--to`, bounded `export --all-available` и offline `reconcile`. Разделяет OAuth App ID и ID клиента АТС, проверяет user/client scope и ownership данных; секреты не выводит. Границы времени и metadata страниц проверяются до commit.

Данные страницы и checkpoint фиксируются одной SQLite-транзакцией. Private raw page атомарно сохраняется раньше checkpoint; производные calls/legs NDJSON и manifest восстанавливаются из БД. Один каталог связан с компанией/host/client/app fingerprint; .lock не допускает двух writers. Пустые окна не доказывают retention, короткая непустая страница не заканчивает обход, failed pages не двигают checkpoint. 429 сохраняет deadline без часового sleep, bounded401 получает новый grant.

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1 — read-only API/OAuth/config | 874ca1d | CLI, safe HTTP/identity, env template и инструкция |
| 2 — durable export/report | 874ca1d | windows/pagination/checkpoint/NDJSON, tests и contract |

## Verification

- `python3 scripts/telphin-spike.py --help` — PASS, без credentials и сети.
- `python3 -m unittest discover -s scripts/tests -p 'test_telphin_spike.py'` — **50 tests, OK, 0.158 s** после последнего исправления.
- Дополнительно проверен вызов `probe` без локальной конфигурации: безопасный JSON `invalid_organization_id`, exit 1, без network request и traceback.
- `git diff --cached --check` — PASS перед implementation commit.
- Fixtures явно synthetic/documentation-derived/live-unverified.

Suite проверяет HTTP/gzip/expiry/bounded401/403/429/5xx/IncompleteRead, одинаковые timestamps и clamp per_page, transfer/CDR/без записи, UTC/fractional boundaries, malformed response до checkpoint, crash до и после commit, resume binding/параметры, private files/symlinks/locking, output redaction, reference reconciliation и unknown lower bound.

## Deviations

- SQLite используется как локальный durable checkpoint store; это Python stdlib, не production БД платформы. NDJSON/manifest производны от SQLite и не служат источником checkpoint.
- Обе тесно связанные CLI-задачи собраны в один implementation commit вместо раздельных коммитов на task.
- Автоматические задачи 01-02 (failure fixtures и reconcile/report) выполнены вместе с CLI; live-часть 01-02 и UAT не завершены.

## Remaining Live Gates

По отдельному указанию пользователя «Сначала заверши CLI без живого подключения» запросы к реальной АТС не выполнялись. Наличие доступа подтверждено пользователем, но credentials не настраивались. TEL-01/TEL-02 и M0 не помечаются live-accepted: нужны фактические probe/history, scope, pagination/retention и независимая сверка. Технически `completed_requested_range` означает завершённый обход, не доказанную полноту провайдера.

## Self-Check: PASSED

Код и документы созданы, offline suite проходит, commit существует. PASS относится к завершению CLI и автоматическим проверкам, не к live-интеграции/полноте истории.
