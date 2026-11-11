# Контракт Телфина: documentation-researched, live-unverified

2026-11-11. Локальная конфигурация и реальные API-запросы ещё не проверены. Этот документ разделяет подтверждённый публичный контракт и гипотезы прототипа.

## Подтверждено официальными источниками

| Область | Контракт | Источник |
|---|---|---|
| Trusted OAuth | POST /oauth/token, form-urlencoded client_credentials; expires_in, Bearer; повторный grant при expiry/ошибке | [OAuth](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1981874187) |
| Пользователь | GET /api/ver1.0/user/: client_id / extension_group_id / extension_id, access full/read_only | [User](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901920682/user) |
| История | GET /api/ver1.0/client/{client_id}/call_history/: page/per_page/order/call_history, call_uuid, cdr; UTC *_gmt | [History](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901921310/call_history) |
| Call/CDR | result whole call и result плеча различаются; устойчивый отдельный ID CDR не описан | [CDR](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901921567/cdr) |
| Лимиты | gzip; RateLimit headers, Reset в секундах, общий часовой бюджет клиента | [API rules](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901920568/API) |
| Запись | GET /client/{id}/record/{uuid} — binary; storage_url endpoint возвращает record_url. MIME может быть application/octet-stream, filename содержит PII | [Record](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901921281/record) |

## Семантика результата и разговора с сотрудником

Whole-call enum: `busy`, `answered`, `bridged`, `not answered`, `rejected`, `failed`. `answered` может означать только ответ IVR/очереди; `bridged` подтверждает соединение с оператором. CLI считает эти результаты раздельно, неизвестные значения сохраняет raw. [Официальный calls/result](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1930231842/calls).

В правилах перезвона нужен отдельный `human_connected`: true для `bridged` либо отдельно доказанного соединения с сотрудником. Один `answered`, наличие записи или CDR answered без подтверждения роли сотрудника не закрывают обращение. Минимальная длительность проверяется по времени соединения с сотрудником, а не по общей длительности IVR/очереди.

## Решения CLI, требующие live-проверки

- Пример документации начинает page с 1 и не содержит total/has_more. CLI завершает окно только на пустом массиве, проверяет echoed metadata, защищён от повторения страниц и изменения effective page size. Это консервативное правило обхода, ещё не доказанный конец на конкретной АТС.
- В запросах order=asc и UTC datetime с точностью секунд; init_time_gmt должен лежать внутри lower..upper. Точную inclusivity, применяемое поле времени, допустимую длину интервала и поддержку сортировки проверить. В прототипе необъяснённое расхождение — ошибка, не пропуск строки.
- `max_pages` хранится отдельно от immutable spec как execution budget: его увеличение при resume не меняет набор данных и не повторяет уже committed pages. Расширение `max_windows` меняет временной охват и требует нового каталога.
- Local per_page=100 и max=1000 — настройки прототипа, не заявленные лимиты провайдера. Ограничение диапазона списка record не переносится на call_history.
- Текущий logical key — call_uuid; каждое отличающееся содержимое отмечается в variants. CDR хранится в raw snapshot и отдельном NDJSON с ordinal; это не гарантированная identity плеча. На переводах/очередях подтвердить cardinality и соответствие webhook CallID.
- Дедупликация и фиксированный end не устраняют дрейф offset pagination при позднем появлении/изменении строк. Полнота требует контрольного периода и повторного сканирования в новый каталог. Production polling/reconciliation — фаза 3.
- Успешный user-level client probe не доказывает наличие права истории или глубину хранения. scope=all не отменяет ограничений группы/добавочного. Данные каждой страницы дополнительно проверяются по client_owner_id.

## Обязательные live evidence перед закрытием M0

- Локальные credentials конкретной компании; проверенные host/client membership и права истории.
- Контрольный период с входящим/исходящим/пропущенным/без записи/переводом/внутренним, если эти случаи доступны на АТС.
- Фактические page/per_page, критерий конца, диапазоны/UTC boundaries, depth/retention, несколько одинаковых timestamps и late updates.
- Одна доступная запись: binary/MIME/codec/mono-stereo, готовность и доступность без публикации file/filename/URL. Аудиоскачивание в текущий CLI не добавлено; для этой части остаётся ручная проверка/следующий media adapter.
- Независимая контрольная выгрузка или сопоставление UUID, объяснение всех расхождений и подтверждение пользователя в UAT. Машинный matched reference сам по себе не подтверждает источник.

Сейчас verified live calls = **не измерено**, verified full-history lower bound = **unknown**. Включение реальных API-прав подтверждено пользователем; фактический probe и сверка не выполнены.
