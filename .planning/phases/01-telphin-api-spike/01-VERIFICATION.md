---
phase: "01-telphin-api-spike"
status: human_needed
offline_cli_status: passed
live_api_status: deferred_by_user
verified_at: "2026-11-11"
verifier_role: gsd-verifier
verifier_agent: telphin_spike_verifier
tests_passed: 50
tests_failed: 0
m0_closed: false
---

# Phase 1: проверка реализации CLI

**Offline CLI: PASSED. Фаза 1 в целом: HUMAN NEEDED.** Реализованный прототип проверен на синтетических ответах и реальной локальной файловой системе. Реальные звонки не получены, полнота истории и M0 не подтверждены.

Пользователь явно выбрал: **«Сначала заверши CLI без живого подключения»**. В рамках этой проверки не выполнялись запросы к аккаунту Телфина, чтение credentials или аудио клиентов. Live-проверка отложена, а не засчитана успешно.

## Проверенные артефакты

- `scripts/telphin-spike.py`: stdlib CLI с командами `probe`, `export`, `reconcile`.
- `scripts/tests/test_telphin_spike.py`: 50 offline-тестов с fake transport, SQLite, файловой системой и авариями.
- `scripts/tests/fixtures/telphin/history.json`, `errors.json`: синтетические documentation-derived fixtures, не live evidence.
- `.env.example`, `docs/telphin-spike.md`, `docs/provider-contract.md`: локальная настройка, команды, коды завершения и разделение проверенного публичного контракта от live-неизвестных.
- `01-CONTRACT-UPDATE.md`: актуализация официального API-контракта перед исполнением.

## Фактическая проверка

Независимый verifier выполнил:

```bash
python3 -m unittest discover -s scripts/tests -p 'test_telphin_spike.py'
```

Результат: **Ran 50 tests in 0.164s — OK**. Прикладные вызовы сети заменены синтетическим transport; тестовые идентификаторы и содержимое не принадлежат клиентам.

| Область | Проверенное поведение |
|---------|----------------------|
| OAuth и scope | Form-urlencoded trusted grant, gzip, expiry, один повтор после 401, 403 без бесконечного retry, запрет extension/group scope и чужого client ID; `scope=all` не заменяет проверку `/user/` |
| Transport | HTTPS allowlist, блокировка redirects, ограниченные network/5xx retries, безопасный malformed gzip/HTTP body failure, нечисловой JSON отклонён |
| Пагинация | Три страницы с одинаковым временем, server-clamped page size, обход короткой страницы до пустой, echoed metadata, repeated-page/page-size guards, явный max-pages failure |
| Данные | Нет фильтра по направлению/результату/записи; whole-call result отделён от CDR; ownership, UTC boundaries/fractional seconds и типы полей проверяются до checkpoint |
| Durability | Данные страницы и checkpoint в одной SQLite-транзакции; raw source атомарно до commit; crash до/после commit не пропускает и не дублирует вызовы при resume |
| Изоляция | Каталог привязан к organization/client/host/app fingerprint; смена binding/параметров при resume запрещена; файлы mode 600, каталог mode 700, exclusive lock и symlink checks |
| Лимиты и полнота | Reset сохраняется без сна на час; persisted deadline блокирует запросы resume; бюджет all-available соблюдается, пустые окна не доказывают retention, результат остаётся partial/unknown |
| Сверка | Missing/extra/CDR mismatch и незавершённые окна влияют на результат; matched reference не повышает автоматически human/live verification |
| Диагностика | Provider body, номера, login, токены и storage URLs не попадают в обычный вывод успешных команд и обработанных ошибок |

## Исправления по независимому review

Во время проверки устранены и покрыты regression-тестами:

1. Отсутствовавшая проверка `init_time_gmt` и границ окна: неверная страница теперь отклоняется до commit.
2. Необработанный `zlib.error` при повреждённом deflate: безопасный `invalid_json_response`.
3. Необработанные `http.client.HTTPException`/обрывы тела: ограниченный retry и безопасная диагностика.
4. Некорректные формы optional fields, способные отравить checkpoint и сломать snapshot: отклонение до commit; счётчики также защищены по типу.

Неразрешённых существенных дефектов offline CLI по проверенному scope не обнаружено. Это не аудит production-коннектора, серверной авторизации или браузерной платформы будущих фаз.

## Покрытие требований и оставшиеся gates

| Требование | Offline evidence | До закрытия требования |
|------------|------------------|------------------------|
| TEL-01 | OAuth/probe, scope/ownership, безопасная конфигурация и ошибки реализованы и протестированы | Реальный probe и подтверждение прав конкретной АТС |
| TEL-02 | Пагинация, все исходы/направления, CDR, дедупликация, окна, resume и partial coverage реализованы | Реальная выгрузка; проверенные pagination/time boundaries/depth/cardinality |
| TEL-03 | Машинная сверка и обезличенный отчёт, failure cases и contract document готовы | Независимый контрольный период из кабинета/разрешённой выгрузки, объяснение расхождений и UAT |

**M0 остаётся открытым.** Отдельно требуются реальная доступная запись и проверка MIME/codec/каналов; аудиоскачивание не входит в текущие команды CLI и описано как оставшаяся manual/media-проверка.

Не доказаны доступная нижняя граница истории, максимальный provider per_page, точный критерий конца, inclusivity/поле временного фильтра, UUID/CDR cardinality и влияние late updates на offset pagination. Полнота не выводится из отсутствия старых строк. Текущие calls/legs представляют последнее наблюдение, исходные варианты сохраняются в raw-pages.

Перед будущей live-проверкой использовать отдельные локальные credentials и каталог выбранной компании. Поскольку пользователь отложил подключение, в текущем шаге дополнительные ключи и live-действия не запрашиваются.
