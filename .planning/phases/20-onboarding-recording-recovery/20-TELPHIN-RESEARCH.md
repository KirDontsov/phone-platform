# Phase 20 — Telphin recording / queue research

Дата: 2026-11-13. Область: собственная Phone production, только затронутая компания. Исследование read-only: SQL aggregates и file metadata; новых запросов в Телфин, принудительных retries, прослушивания аудио и изменений production не было. Идентификаторы компании, пути записей, телефоны и секреты не включены.

## Подтверждённое состояние

Снимок 08:45:55 UTC:

| Метрика | Количество |
| --- | ---: |
| Импортированные звонки | 43 370 |
| Звонки с идентификатором записи у провайдера | 31 979 |
| Отдельные recording documents | 33 933 |
| `ready=true`, `state=ready`, непустой path | 5 187 |
| Из ready с сохранившимся старым `last_error=provider_rate_limited` | 3 591 |
| `pending`, `ready=false`, rate limited, без path | 28 746 |
| Ready с повреждённым/non-ready state | 0 |

Все 5 187 путей ready проверены внутри собственного API-контейнера через `test -f`/`test -s`: 5 187 файлов существуют и непустые, отсутствующих 0. Содержимое файлов не читалось. Из текущих ready 1 596 имеют последнее изменение вчера. Оснований заявлять об удалении ранее скачанных записей нет.

Агрегирование до уровня звонков: 4 933 звонка имеют хотя бы одну готовую запись, ещё 27 046 — только ожидающие загрузки. У 3 448 из 4 933 готовых звонков есть старый rate-limit error на одной из записей, но `RecordingAvailability` отдаёт им `ready`.

## Причина и границы вывода

`has_recording` при импорте означает `record_uuid` в истории Телфина, а не скачанный файл. `Лимит Телфина` означает отложенное скачивание из-за HTTP 429/сохранённого cooldown. Это не признак удаления у провайдера и не ошибка воспроизведения локального файла.

В `phone-api/src/calls.rs` READY уже имеет приоритет над rate-limit. Media endpoint читает локальный файл после tenant/parent/path проверки и не обращается к Телфину. `import_call` создаёт recording только при отсутствии документа; повторный импорт не заменяет ready/path. `recording()` выходит по `ready=true` до чтения подключения и backoff. Поэтому текущий код/данные не подтверждают регрессию «готовая скачанная запись стала Лимит Телфина». Идентичность конкретной строки, которую пользователь видел вчера, без call ID не проверена; историческое наличие `record_uuid` само по себе не доказывает скачивание.

Подтверждена отдельная серьёзная ошибка алгоритма очереди: `claim_job` выбирает `ORDER BY next_attempt_at,created_at` без учёта общего cooldown и приоритета истории. Каждые 2 секунды worker снова забирает одну из десятков тысяч recording jobs, даже если подключение уже заблокировано до часового сброса. `connection_config` возвращает сохранённый rate-limit, что расходует очередную attempt, переписывает document error и connection cooldown, хотя HTTP не выполнялся. Каждый раз deadline вычисляется через остаток секунд; полного часа на каждый job код не добавляет, но шумные повторные записи/расход попыток есть.

Оригинальный sync по-прежнему `retry`, attempts=2, next_attempt_at=00:00:08 UTC, последнее изменение вчера 23:39. Снимок 08:46:55 UTC: 10 119 due jobs этой компании стоят перед ним по текущей сортировке; sync просрочен на 31 607 секунд. Retention также стоит с 00:00. При этом recording jobs постепенно успешно скачиваются между настоящими часовыми лимитами. Это starvation, а не отсутствие автоматического worker. Фактический общий deadline снимка — 09:00:06 UTC. Точную оставшуюся длительность/размер очереди UI сейчас не показывает.

`record_failure` способен выставить recording state pending/failed без проверки готовности, хотя ready/path не стирает; текущих downgraded ready documents не найдено. Успешное скачивание не удаляет старый last_error — подтверждено на 3 591 записях. Нужна защитная очистка ошибки и инвариант сохранения готовности.

## Минимальный fix

1. В claim SQL исключать sync/recording компании с будущим `connection(kind=telphin).next_request_at` до claim, без расхода attempts и без provider calls. JSON проверять типизированно: `jsonb_typeof(c.data->'next_request_at')='number' AND c.data->'next_request_at' > to_jsonb(floor(extract(epoch FROM now()))::bigint)`; не приводить пользовательский JSON text к bigint. Ключ connection ограничен организацией candidate job.
2. Дать due sync приоритет перед recording; purge/retention также не должны ждать завершения всего audio backfill. Сохранить SKIP LOCKED, lease/fencing, state/due/max-attempt guards. Одна сортировка по дате не обеспечивает нужный порядок.
3. Различать настоящий ответ HTTP 429 и использование сохранённого backoff: последнее не должно переписывать глобальный deadline. Для race после claim оставить защиту connection_config. При сохранении deadline не сокращать действующее более позднее окно.
4. При успехе записи удалить старый last_error; при ошибке не понижать ready/path. Ранний `ready` check согласовать с API, принимающим и `ready=true`, и `state=ready`.
5. В API/UI показывать, что ограничено именно скачивание, время возобновления и отдельную готовность записи. Не маскировать READY глобальным состоянием подключения. Исходную sync-job/checkpoint сохранять; не начинать новый all-history import и не сбрасывать attempts без доказанной необходимости.

## Проверка реализации

Изолированный локальный PostgreSQL 16 в отдельном Docker-контейнере/новой временной БД, `TEST_DATABASE_URL` только на него. Не использовать production для fixture. `crates/persistence/tests/postgres.rs` уже имеет ignored actual-PG harness (`cargo test -p persistence --test postgres -- --ignored`), worker `tests/pipeline.rs` использует `TEST_DATABASE_URL` и serial mutex. Новым тестам нужны isolated fixture orgs и cleanup/transaction: blocked sync+recording остаются untouched; другая компания/другой kind продолжается; due sync и retention опережают старые recording; malformed/missing JSON не вызывает cast error; due/max-attempt/lease/fencing не ослаблены. Worker regression проверяет сохранённый cooldown vs actual429 и ready/path/error инварианты без сетевых запросов к провайдеру. Основные новые тесты проверить RED на исходном коде, затем GREEN после fix.

Часовой X-RateLimit-Reset — секунды до сброса, уже подтверждено исследованием `.planning/research/TELPHIN.md` с официальной документацией API. Без нового provider-запроса нельзя подтвердить текущий точный тарифный бюджет/Remaining; исследование его не выдумывает.
