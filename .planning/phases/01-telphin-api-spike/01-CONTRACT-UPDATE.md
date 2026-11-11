# Phase 1: уточнение контракта перед реализацией

2026-11-11. Проверены публичные официальные страницы и публичный Confluence `body.storage`. Live-запросы к АТС, credentials и записи клиентов не использовались.

## Авторизованный пользователь

`GET /api/ver1.0/user/` возвращает `client_id`, `extension_group_id`, `extension_id`, `access`, `admin`, `id`, `login`, `timezone`. Ненулевой `client_id` обозначает пользователя уровня клиента; группа/добавочный имеют собственные поля. `access=full/read_only` описывает разрешённые операции, а не охват АТС. `read_only` подходит для импорта; OAuth `scope=all` сам по себе не доказывает доступ ко всей компании. Проверять соответствие ожидаемого client ID и результата `/user/`; login не выводить. При группе/добавочном без доказанного client-level scope не заявлять полную историю АТС. [Пользователь](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901920682/user), [уровни доступа](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901920377).

OAuth App ID относится к `POST /oauth/token`, а provider client ID — к АТС. Trusted grant: `client_credentials`; ответ содержит Bearer token, expiry и scope. [Trusted OAuth](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1981874187).

## История и CDR

`GET /api/ver1.0/client/{client_id}/call_history/`; отдельный звонок — `/call_history/{call_uuid}`. Пример использует `page=1`, `order=desc`, `per_page=100000`. Это не гарантия максимального размера. Ответ: `page`, `per_page`, `order`, `call_history`; отдельного `total/has_more` в примере нет. Элемент имеет `call_uuid` и массив `cdr`. UTC-времена: `init_time_gmt`, `start_time_gmt`, `bridged_time_gmt`, `hangup_time_gmt`; формат примера `YYYY-MM-DD HH:MM:SS`. Query использует `start_datetime/end_datetime` того же вида.

Публичный пример содержит многоточие и завершающую запятую: документационный fixture надо нормализовать, явно пометив `documentation-derived/live-unverified`. Устойчивого отдельного CDR ID не описано; не назначать выдуманный provider leg ID. Сохранять плечи как исходный массив/snapshot. `record_uuid` nullable. Result всего звонка отличается от result плеча: `bridged` принадлежит вызову, `answered elsewhere` — плечу. [История](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901921310/call_history), [CDR](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901921567/cdr).

## Записи

`GET /api/ver1.0/client/{client_id}/record/{record_uuid}` возвращает файл; пример имеет `Content-Type: application/octet-stream`, filename в `Content-Disposition` может содержать телефоны. Не выводить filename. `GET .../record/{record_uuid}/storage_url/` возвращает JSON с ключом **`record_url`**; постоянная ссылка не требует авторизации и должна оставаться закрытой. Формат/каналы, поддержка HEAD/Range не доказаны документацией. [Записи](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901921281/record).

## Ограничения реализации spike

- Не заканчивать пагинацию по `len(items) < requested_per_page`: сервер может ограничить фактический размер. Читать до пустой страницы с проверкой echoed page/per_page, ограничением max pages и защитой от повторяющихся страниц. Точный контракт окончания всё ещё проверяется live; 400/404 не считать концом автоматически.
- Длина окна call_history, включённость границ, фактический лимит страницы и retention остаются unknown. Ограничение диапазона списка record не переносить на call_history.
- `--all-available` ограничить числом окон/явной нижней границей. Пустые старые окна не доказывают начала истории: при неизвестной нижней границе manifest остаётся partial/unknown_lower_bound.
- План достаточен для offline реализации. Документальные fixtures и probe без сверки не закрывают TEL-03/M0. Сначала подготовить весь CLI, затем подключить имеющийся доступ локально и получить реальные evidence.
