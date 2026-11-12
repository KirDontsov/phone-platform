---
phase: "11-configuration-assistant"
status: PASS
review_round: 2
revision_count: 1
revision_limit: 3
reviewed_at: "2026-11-12"
reviewer: independent_plan_checker
implementation_verified: false
---

# Phase 11 — независимая проверка плана, раунд 1

Проверены `11-RESEARCH.md`, три PLAN, `11-REVISION-LOG.md`, `docs/contracts/assistant-v1.md`, `docs/assistant-ui.md`; сопоставлены read-only исходники Bondscan ai_chat/MCP и действующие settings/auth сервиса. Секреты, `.env` и auth.json Bondscan не читались. Реализация фазы ещё не начата. Первоначальный результат был **BLOCKED до доработки планов/контракта**. После завершённой доработки 1 и независимого повторного чтения текущий результат — **PASS, 0 открытых P1/P2**. Исходные findings ниже сохранены как история review; это не новая просьба о разрешении.

Версии и способы авторизации сверены с официальными [Codex app-server](https://learn.chatgpt.com/docs/app-server), [authentication](https://learn.chatgpt.com/docs/auth), [MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli). Это подтверждает доступные протокольные механизмы, а не работоспособность будущего контейнера или entitlement конкретного аккаунта.

## Что уже спроектировано достаточно конкретно

- Отдельный assistant runtime и stdio SDK bridge; analysis runner остаётся deny-all/MCP0.
- Owner + original live session + company + own-user conversation, токен capability вне prompt; отсутствие org/user/session в tool arguments.
- Восемь узких конфигинструментов, прямые общие API/domain services вместо generic HTTP/SQL; исключены IAM, retention, export и произвольный network/shell.
- Secure cards и opaque credential handles; нет обещания обнаруживать любой opaque secret regex-ом.
- Durable run/message/events и polling, строгий terminal status вместо EOF-success; mutation/receipt/audit задуманы атомарно; cancel не обещает rollback.
- Реальные локальные CLI/SDK/PG/Docker/browser проверки отделены от внешнего Codex/Телфина и пользовательского UAT. Auth UI не выдаёт model catalogue за доступную модель.

## Исходные findings — история первого раунда

### P1-01 — запрет builtin capabilities не имеет исполнимой negative-проверки

**Где:** `11-02-PLAN.md:17–20`, `11-03-PLAN.md:18`; research прямо говорит, что flags ещё не доказательство. Verify пока перечисляет version/initialize/schema/account-unconfigured/config и SDK tools/list/call. Это проверяет конфиг и bridge, но не набор инструментов, фактически предлагаемый Codex executor, и не способность обойти запрет встроенным инструментом.

**Риск:** контейнер read-only и `network=false` не заменяют запрет shell/read/edit: runtime auth home доступен процессу, а shell/host RPC могут иметь отдельную семантику. PASS только по flags оставит центральную границу недоказанной.

**Требуемая доработка:** записать обязательный actual pinned Codex negative probe на локальном Responses-compatible fixture backend без внешних аккаунтов/платных inference. Fixture фиксирует предлагаемый tool catalogue и навязывает shell/file-write/file-read/web/agent или неизвестный tool call. Проверить отсутствие выполнения и canary filesystem/network эффектов; список доступных инструментов — только фиксированные MCP capabilities. Отдельно проверить отказ wrapper для arbitrary RPC вроде `thread/shellCommand`, подмены MCP config/URL/command и неизвестного capability. Если pinned runtime не даёт доказуемо запретить конкретный builtin, запуск assistant должен fail closed, а не только сообщить warning.

**Приёмка плана:** точная команда/артефакт probe, ожидаемые negative outcomes и fail-closed критерий в `11-02`/контракте. Fake Codex wire unit tests остаются дополнительными, не заменяют actual executor probe.

### P1-02 — delayed device login не привязан к живой authority инициатора

**Где:** контракт `assistant-v1.md:17–19,63–69`, `11-01-PLAN.md:17`, `11-02-PLAN.md:17`. Original session связана с run capability, но private auth start/status/cancel/logout получает только organization_id; lifecycle pending login описан как per-org serialization. Не определено, кто и при каких проверках продвигает delayed login completion в активный auth home.

**Риск:** owner начинает device flow, затем logout/switch/role revoke; external completion позже сохраняет credentials в общей company auth state без актуальной authority. Старое completion также может затереть новый API-key login/logout.

**Требуемая доработка:** у pending auth attempt должны быть server-side org/user/original-session/login ID + монотонная auth generation/expiry. Перед активацией credentials заново проверять живую исходную owner-сессию и generation; stale completion уничтожается и не меняет current auth. Зафиксировать отмену/отзыв authority и policy auth logout относительно уже идущих turns/auth snapshots. Нельзя передавать raw browser cookie в runner или делать generic auth RPC ради этой проверки.

**Приёмка плана:** tests start→logout/switch/revoke→late completed; start A→new start/logout B→late A; чужой login ID; auth logout во время active turn. Ни один stale ответ не публикует authenticated=true и не восстанавливает отозванный auth.

### P1-03 — revocation/cancel и mutation не имеют определённой точки сериализации

**Где:** контракт `assistant-v1.md:7,29,57`, `11-01-PLAN.md:18`. Transaction указан как capability/session check → row/advisory locks → mutation. Не определены общие lock targets/order и повторная проверка после получения блокировок.

**Риск:** call проверил live capability, затем ждёт lock; cancel/logout/removal успел подтвердиться, после чего старый call всё-таки применяет изменение. Последовательные negative tests «вызвать после cancel» этой гонки не обнаруживают.

**Требуемая доработка:** явно определить linearization для run revoke и write: одинаковый run lock, повторная проверка run/token/session/owner после получения нужных locks, согласование с удалением сессии/изменением membership и конечная запись receipt. Mutation, успешно сериализованный до cancel, остаётся committed; mutation после подтверждённого cancel/revoke не применяется. Не держать DB transaction через model/provider HTTP. Указать lock order или допустимый retry при deadlock/conflict.

**Приёмка плана:** двухсоединительные PG barriers для tool-vs-cancel/delete и tool-vs-session/membership revoke; проверить фактические настройки, receipt и audit, а не только код HTTP.

### P1-04 — все три плана не проходят фактический GSD structure verifier

Выполнены реальные read-only команды установленного инструмента:

```sh
node /Users/kirilldoncov/.claude/get-shit-done/bin/gsd-tools.cjs verify plan-structure .planning/phases/11-configuration-assistant/11-01-PLAN.md
node /Users/kirilldoncov/.claude/get-shit-done/bin/gsd-tools.cjs verify plan-structure .planning/phases/11-configuration-assistant/11-02-PLAN.md
node /Users/kirilldoncov/.claude/get-shit-done/bin/gsd-tools.cjs verify plan-structure .planning/phases/11-configuration-assistant/11-03-PLAN.md
```

Результат каждого: **valid=false**, error `Missing required frontmatter field: must_haves`. Дополнительно у всех семи tasks отсутствуют `<files>` и `<done>` (14 warnings). Поэтому механический PASS сейчас невозможен.

**Требуемая доработка:** добавить проверяемые `must_haves.truths/artifacts/key_links`, task files и конкретный done criterion. Повторить три structure checks; empty must_haves ради формального valid не достаточно.

## P2 — согласование контракта и scope

### P2-01 — optimistic versions для конфигопераций необязательны/отсутствуют

Контракт строки 49,52–55: taxonomy/filter допускают optional version, `connect_telphin`, callback rules и rubric activation не имеют revision precondition. Тест stale version докажет поведение только при наличии поля, не предотвратит blind overwrite при его отсутствии. Это особенно заметно при параллельном ручном редактировании во время model turn.

Зафиксировать conditional requirement: update существующего объекта требует expected revision из workspace snapshot; create без id не обновляет неоднозначный existing object. Определить CAS для connection/org policy/active rubric либо явно сформулировать допустимую scoped merge policy, не перезаписывающую изменённые вручную поля. Tests: missing expected revision, stale revision, concurrent UI update, same-name reuse с отличающимся содержимым. Receipt replay при тех же args не должен повторно применять update.

### P2-02 — UX research обещает отсутствующий resume/credential protocol

`docs/assistant-ui.md` описывает waiting_credentials/uncertain turn, typed credential-request card и продолжение того же run после добавления доступа. Контракт имеет только queued/running/completed/failed/cancelled/interrupted, не содержит resume/continuation endpoint или typed credential-request event; Codex turn ephemeral и bounded180s. В UX API-key также назван отдельным credential_ref, а REST auth/start принимает api_key напрямую в отдельной secure form.

Согласовать UX с v1: либо явно добавить и проверить continuation protocol, либо завершать текущий run безопасным запросом secure card и начинать следующий turn с новым client_request_id/credential_ref. Вариант без continuation достаточен; расширять runtime необязательно. Не реализовывать неописанное waiting state на эвристике текста модели.

### P2-03 — новые requirement IDs не определены, lifecycle секретов описан неполно

Все три плана ссылаются на CHAT-01…03/CODEX-01…02/MCP-01/SETUP-01, но в `.planning/REQUIREMENTS.md` нет этих требований; STATE пока на ready_for_final_uat десяти фаз. До execute нужны definitions/owning phase и текущая фаза, чтобы verifier проверял согласованный scope.

Дополнительно зафиксировать физическую очистку expired/unapplied credential ciphertext, отменённых auth attempts и runtime temp/auth artifacts; lookup denial сам по себе не удаляет секрет. Указать backup/restore boundary для нового auth volume и restored pending runs/handles: либо auth cache не входит в backup и требует re-login, либо отдельная защищённая процедура; restored revoked sessions не оживляют authority. Для каталогов нужны0700, для auth files0600 — формулировка «per-org0600 auth home» сейчас неоднозначна. Это можно оформить одной компактной lifecycle/traceability доработкой без новых features.

## Revision loop

Это **первый независимый раунд**, revision cap — 3. Исполнитель дорабатывает только указанные plan/contract gaps и записывает изменения в `11-REVISION-LOG.md`; затем повторяется independent check. Нет необходимости запрашивать у пользователя повторное разрешение на уже согласованные конфигдействия или останавливать независимую разработку из-за отсутствия внешних credentials. При отсутствии прогресса после предусмотренных итераций план остаётся BLOCKED с конкретной причиной.

Текущий подсчёт: **4 P1, 3 P2; execute gate BLOCKED**. Это оценка плана. Никакая часть реализации, actual negative probe или live авторизация этим документом не объявляется выполненной.

## Проверка доработки 1 — промежуточный delta

После написания основного review root представил revision 1. Повторно прочитаны обновлённые contract/plans и согласованный frontend UX.

- P1-01 закрыт **на уровне плана**: реальный pinned Codex + offline Responses negative executor probe стал обязательным permanent test. Сообщённый researcher эксперимент не засчитывается этим документом как готовый production runner; в execute нужно применить тот же runtime/MCP конфиг и проверить отсутствие forbidden builtin/host RPC capabilities.
- P1-02 закрыт на уровне плана: staged auth, original session/user, generation/TTL, транзакционный `/auth/complete`, stale completion deny, private pending code и recovery после lost cleanup определены.
- P1-03 закрыт на уровне плана: org → conversation/run locks, повторная live authority check под locks, синхронизация с session/membership mutation и такая же cancel ordering определены. Двухсоединительные PG tests остаются implementation acceptance.
- P2-01 закрыт: mandatory expected_revision, HTTP/shared service settings_revision, version для id updates и409 при отсутствии/устаревании стали контрактом.
- P2-02 закрыт актуализацией `docs/assistant-ui.md`: polling, контрактные states, новый обычный turn после secure card; API key отдельный auth endpoint, не credential_ref.
- P1-04 пока открыт: must_haves/files/done ещё отсутствуют во всех трёх PLAN.
- P2-03 пока открыт: definitions новых requirement IDs/текущая phase и компактный secret expiry/restore lifecycle пока не добавлены.

Текущий статус остаётся **BLOCKED**, осталось **1 P1 + 1 P2** из baseline. Это часть первого revision-loop; дополнительная новая итерация за параллельное уточнение findings не расходуется. Формальный повтор structure checks выполняется после завершения доработки оставшихся пунктов.


## Финальная проверка завершённой доработки 1 — PASS

Независимо повторно прочитаны contract, все три PLAN, lifecycle, UX, STATE и новые requirement definitions. Реально повторены три `gsd-tools verify plan-structure`: **3/3 valid=true, errors0, warnings0**; проверены все **7 tasks**, у каждого files/action/verify/done. Must-haves содержат предметные truths/artifacts/key-links, не пустые placeholders. План wave1 API/runner и wave2 UI/integration согласован с фиксированным wire-контрактом.

| Finding | Итог доработки 1 |
| --- | --- |
| P1-01 executor negative proof | В контракте и 11-02 обязательный actual pinned Codex + offline Responses forced-execution probe; config flags/fake Codex не заменяют его |
| P1-02 delayed auth authority | Original session/user/org/TTL/generation, staged promotion через API authorization transaction, stale callback deny и recovery определены |
| P1-03 cancel/revoke ordering | Org → conversation/run locks, повторная live session/capability/membership check под locks; cancel использует тот же порядок |
| P1-04 GSD structure | Три structure проверки valid; содержательные must_haves и все task files/done добавлены |
| P2-01 CAS | mandatory expected_revision/settings_revision через общие HTTP services; id update требует version; missing/stale409 |
| P2-02 UX contract drift | UI polling, контрактные states, отдельный API-key auth endpoint и новый turn после credential card; неописанный resume исключён |
| P2-03 traceability/lifecycle | 7 новых requirement definitions, STATE phase11; minute GC/consumed ciphertext erasure, dirs0700/files0600, auth volume excluded from backup, restored authority invalidated и deleted assistant subjects ledger описаны |

**Execute gate открыт.** Израсходована **1 доработка из максимум3**; второй формальный review — повтор этой доработки, не «доработка2». Никаких повторных подтверждений типизированных разрешённых операций у пользователя не требуется.

PASS относится только к достаточности плана. Implementation verifier по-прежнему обязан проверить:

- настоящие двухсоединительные PG race barriers tool-vs-cancel/delete/session/membership revoke; mutation/receipt/audit и replay/revision;
- actual executor tool catalogue/forced forbidden calls с тем же runtime/MCP конфигом, отказ произвольного host RPC; model requests только к локальному fixture, не внешние платные;
- реальный положительный SDK tools/list/call и конфигизменения временной компании, tenant/owner/original-session guards;
- late auth completion/generation/cancel/logout и отказ stale auth cache после lost cleanup/restore;
- физическое уничтожение credential ciphertext/cleanup, расширенный backup/deletion-ledger regression;
- desktop/mobile/browser UX, secret sentinel absence в модели/history/events/audit и честные unavailable/terminal states.

Существование исследовательского negative probe не делает production runtime проверенным. Настоящая ChatGPT device approval, доступ/качество конкретной модели и реальный Телфин остаются отдельными live gates после локальной настройки владельцем. Следующая стадия — execute трёх планов, затем независимый implementation verifier и UAT.
