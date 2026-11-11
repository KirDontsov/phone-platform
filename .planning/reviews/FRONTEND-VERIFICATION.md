# Frontend implementation verification

Дата: 2026-11-11. Область: `phone-client`; контракт `docs/contracts/platform-v1.md`. Это проверка реализованного frontend, не подтверждение качества планов и не закрытие live-пилота. Полный пользовательский UAT ещё предстоит; сценарий — `docs/uat.md`.

## Проверенный результат

Vue 3/TypeScript/Vite/Pinia/router/Tailwind SPA работает с настоящим API через cookie-сессию. Реализованы авторизация/reset/invite, переключение изолированных компаний, дашборд, журнал/поиск/пагинация/AST-фильтры/представления, карточка/Range-аудио/версии/доказательства/ручная разметка/роли, контроль перезвонов, настройки, подключения, интеграции, задания/аудит/расход.

Неподключённый внешний провайдер не получает подставного успеха. Синтетические данные выдаёт backend только в явном demo-режиме. Frontend не хранит session token в localStorage; API client передаёт CSRF и активную компанию, отменяет запросы при смене tenant и отклоняет поздний ответ старого поколения. Сервер остаётся источником проверки прав.

## Автоматические доказательства

| Проверка | Фактический результат |
| --- | --- |
| `npm --prefix phone-client test` | 20/20 PASS, 7 файлов; AST/ограничения, tenant cancellation/late responses/CSRF, конструктор, роли, nullable стоимость и архивная разметка |
| `npm --prefix phone-client run build` | PASS: vue-tsc без ошибок, Vite production build; основной JS 108.43 kB / gzip 42.91 kB |
| Реальный API + PostgreSQL + Mailpit, первоначальный E2E | 10/10 PASS, без skips: auth/refresh/logout/reset non-enumeration, переключение компаний и недоступность старого call URL, ручные правки, owner screens, reviewer, ошибка switch, optimistic409, invitation/reset через настоящее письмо |
| Реальная очередь + worker, focused E2E | 1/1 PASS: reprocess через UI, новые версии transcript/analysis по результату worker |
| Mobile regression E2E | 1/1 PASS: dashboard/settings/call transcript/call analysis; document.scrollWidth <= viewport, карточки внутри viewport |
| Новый реальный roles E2E | 1/1 PASS (4.2s): `tests/e2e/transcript-roles.spec.ts`; новая known immutable transcript + job_id, старые segments и защищённые ручные поля сохранены; исходная uncertain версия доступна |
| Финальный повтор E2E на Docker nginx | 13/13 PASS, zero skips, 44.1s, `--workers=1`, `http://127.0.0.1:18081`: финальные Docker nginx/API/worker + PostgreSQL/RabbitMQ/Mailpit |

Финальный повтор всех 13 сценариев выполнен на Docker18081: все обязательные E2E env заданы, skips0; один worker сериализует изменения общих demo fixtures. Тесты первоначально выполнялись на `http://127.0.0.1:18082` (Vite proxy → API18080), с изолированной локальной PostgreSQL и синтетическими demo-учётными записями. В smoke/default тестах отсутствие E2E env обозначается skip; приведённые выше проходы использовали обязательные env, skip не учитывался как PASS. Единственная преднамеренная HTTP-подмена — сценарий503 при смене компании, проверяющий восстановление селектора; остальные перечисленные E2E работают с реальным API.

## Contract review: формы записи

| UI / действие | API / проверенные поля | Права в UI |
| --- | --- | --- |
| Вход, выход, организация, reset, invite accept | `/auth/*`: email/password, organization_id, token/name; reset/invite token удаляется из адресной строки | Public auth; checked session for switch/logout |
| Ручная разметка | PATCH `/calls/:id`: status_id|null, tag_ids, note, contact_kind, manual_lock, version | Owner/manager |
| Перезапуск и удаление | POST `reprocess` stage; DELETE call с подтверждением | Owner/manager reprocess; owner delete |
| Подтверждение ролей | POST `/calls/:id/transcripts/:tid/roles`: только реальные unique speaker keys и manager/client/unknown; выбранный transcript ID фиксируется при открытии формы | Owner/manager; uncertain transcript |
| Фильтр | POST `/filters` name/ast, DELETE `/filters/:id` | Owner/manager |
| Компания | PATCH `/organization`: name/timezone/callback_minutes/retention_days, callback_allow_inbound/min_duration; working_hours `{enabled:true,days:[1..7],start,end}` либо null | Owner |
| Участники | POST invite email/role; PATCH role/extension; DELETE member/invitation | Owner |
| Таксономия | POST kind/name/color; PATCH name/color/archived; сохранённые архивные значения видны, неназначенные архивные значения исключены из карточки | Owner settings, owner/manager assignment |
| Шкала | POST rubric name/criteria, POST activate; бизнес rubric_version отдельно от row version | Owner |
| Подключения | PUT config, POST test/sync, DELETE disconnect; blank secrets сохраняют существующие; numeric budget/timeout; Telphin extension_mapping object/string values, history_start date + explicitly confirmed history_start_verified; Sheets mapping JSON | Owner |
| OAuth и сделки/экспорт | authorize URL только HTTPS; POST link call/provider/contact/deal; POST export call_ids/provider | Owner configure, owner/manager link/export |
| Очередь и хранение | POST job/retry, POST purge с подтверждением | Owner retry (включая uncertain reconciliation); owner purge |

Результат ревью: устранены два превращения неизвестной стоимости в0 (analysis/operations), `usage.cost` допускает null; явный0 сохраняется как0. `rubric_version` поддерживает числовое значение API. Исторические архивные теги имеют явную пометку. Unit проверяет полный optimistic PATCH payload с архивными ID и сохранением manual_lock, затем исключение снятого архивного тега из новых назначений. Версия формы сохраняется при background polling, поэтому параллельная правка даёт409 вместо молчаливого overwrite.

## Визуальные доказательства

Финальные реальные authenticated screenshots обновлены на Docker nginx18081 (API/worker/PostgreSQL, без mocked routes): `data/eval/screenshots/dashboard-desktop.png`, `call-detail-desktop.png`, `settings-desktop.png`, `call-detail-mobile.png`, `dashboard-mobile.png`, `settings-mobile.png`. JSON: `data/eval/screenshots/frontend-qa.json`. Desktop1440; mobile390. На трёх мобильных страницах viewport390/document390, JS page errors0. Реальный audio element: duration32, readyState4, errornull. Demo-аудио — искусственный сигнал; это проверка доставки/плеера, не ASR.

Обнаруженное mobile min-content overflow исправлено разрешением shrink для grid/flex children, переносом заголовков/бейджей/длинного текста и ограничением audio width. Глобальное сокрытие горизонтального overflow не применялось. Desktop и исправленная mobile карточка просмотрены визуально.

## Воспроизводимость

```bash
npm --prefix phone-client ci
npm --prefix phone-client test
npm --prefix phone-client run build
E2E_BASE_URL=http://localhost:18081 E2E_EMAIL=owner@demo.local \
  E2E_PASSWORD='PhoneDemo!2026' E2E_REVIEWER=reviewer@demo.local \
  E2E_MAILPIT=http://localhost:18025 E2E_WORKER=1 E2E_ROLE_CALL_ID=651a62e5-421f-5c34-8348-b11a5083e3dc \
  npm --prefix phone-client run test:e2e -- --workers=1
```

Для Chromium: `npx playwright install chromium` из phone-client; допустим готовый браузер через `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH`. В среде проверки использован уже установленный Chromium1254; загрузка браузера CDN не понадобилась.

## Открытая приёмка

Реальные Telphin credentials/записи, ASR/LLM/OpenCode model, amoCRM/Google OAuth/delivery, действительная глубина/полнота истории, качество диалога и нагрузочный профиль не подтверждаются frontend demo. Пользовательский UAT и разрешённый пилот сохраняются открытыми; ограничения в `docs/known-limitations.md`. Статусы фаз должны учитывать эти открытые evidence, даже когда локальные автоматические проверки прошли.

Последняя доработка импорта: UI сохраняет дату начала истории и только явное подтверждение её сверки с АТС; без подтверждения поясняет ограниченный поиск за последний год и неполную coverage. Два component-теста проверяют отсутствие автоматического подтверждения, точный PUT config payload и сохранение секрета пустым значением. Placeholder внутреннего runner и пример docs/providers.md используют `http://model-runner:4096` без завершающего slash.

Сопоставление внутренних номеров Телфина полностью настраивается в UI (`extension_mapping`); два component-теста подтверждают чтение существующего JSON, запись строковых значений, очистку в `{}` и запрет malformed/array/null/non-string/пустых ключей/значений до HTTP-запроса.

## Финальные узкие исправления после полного Docker-прогона

Полные 13/13 относятся к сборке до доработки access audit в API и динамического DNS nginx. После них полный прогон не повторяется без новой причины; приглашение и очередь обработки проверяются отдельно на обновлённом target, результат указывается ниже.

nginx использует Docker DNS `127.0.0.11 valid=5s ipv6=off`, variable upstream и полный `$request_uri`: имя API переопределяется во время работы, `/api` и query не теряются. Поведение сверено с официальными [proxy_pass](https://nginx.org/en/docs/http/ngx_http_proxy_module.html#proxy_pass) и [resolver](https://nginx.org/en/docs/http/ngx_http_core_module.html#resolver). Pinned nginx1.28 `nginx -t`: PASS. Изолированный реальный Docker-тест: backend заменён с IP172.22.0.2 на172.22.0.4 при неизменном frontend container ID; запрос `/api/probe?search=a%2Bb&filter=%7B%7D` вернулся без изменения до/после замены. Тестовые контейнеры и сеть удалены, рабочий стек не затрагивался.

Focused invitation + real worker reprocess после access audit/DNS: **2/2 PASS**, zero skips,14.8s, обновлённый Docker target18081. Отдельный authenticated GET `/api/operations/audit` подтвердил недавние `invitation.created` и `membership.joined` одного и того же приглашения (связь subject_id ↔ details.invitation_id). В вывод проверки не включены email/token. Полные13/13 не повторялись; affected сценарии закрыты на итоговых образах.
