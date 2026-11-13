# Восстановление после ошибочной автоочистки

Команда только для оператора. Она не доступна HTTP API или нейросетевым инструментам.

```
phone-worker recover-retention fetch /recovery/manifest.json /recovery/sources.json
phone-worker recover-retention apply /recovery/manifest.json /recovery/sources.json --dry-run
phone-worker recover-retention apply /recovery/manifest.json /recovery/sources.json
```

В production запускать в одноразовом контейнере собственного worker image, в собственной сети Phone, с runtime env этой установки. Обычный worker остановить на время применения recovery и batch reconciliation. Для manifest/source использовать отдельный приватный volume: каталог0700 и владелец10001, файлы0600. Не включать содержимое source JSON в логи, Git или пользовательский чат.

Manifest — immutable JSON с `organization_id`, `connection_id`, `targets`. Для каждого target обязательны `call_id`, `purge_job_id`, `retention_job_id`, `provider_id`, `purged`, `deleted`, `tombstone`, `manual_audit`, `manual_purge`, `purge_state`. CLI берёт только purged targets, максимум200; проверяет детерминированный Telphin call ID, уникальность, успешное retention-origin purge и отсутствие ручных удалений. Для данного инцидента используется отдельный точный manifest120, полученный из paused snapshot. Не пересохранять manifest после fetch: его SHA256 связывает staging и аудит.

`fetch` проверяет live policy и quota до соединения/каждого запроса; выполняет один scoped GET call_history по каждому UUID, валидирует identity/client/CDR. Каждая полученная запись сохраняется атомарно0600. Повтор использует сохранённые ответы. Свежий429 обновляет общий cooldown организации и останавливает проход; его нельзя обходить резервным host или сбросом даты. Полностью сохранённый stage повторно не обращается к API.

`apply` не обращается к сети. Он требует полный проверенный stage, повторно проверяет live policy, version/tombstone, точный succeeded retention purge и отсутствие ручного `call.deleted` в обеих формах аудита (`action`/`event`). В одной короткой транзакции снимает только ошибочный exact tombstone, возвращает реальные поля через общий Telphin normalizer, создаёт обычные queued recording jobs и запись аудита. Новые recording jobs помечаются manifest hash для приоритетного восстановления; attempts/state/deadline не сбрасываются. Повторное применение того же manifest — no-op.

Восстановленные данные телефонии не означают восстановление старых ручных комментариев/AI-анализов. CLI явно отмечает отсутствие источника этих аннотаций и не создаёт выдуманные значения. Аудио загружает обычный worker с quota guards после resume; поле `audio_downloads:0` в receipt означает, что metadata CLI не скачивал файлы. Готовность аудио подтверждать по actual ready records и непустым файлам, а не по факту metadata apply.

Перед выполнением нужны собственный PG snapshot, immutable manifest, независимое code review и успешные isolated actual-PG проверки. На404/ошибке провайдера оставлять незавершённые targets явно pending. Существующие tombstones ручных удалений и original history checkpoint не изменяются.
