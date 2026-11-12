# Phase11: независимый срез API/runner contracts

Дата: 2026-11-12. Автор этого среза реализовал Node runner, но не root Rust assistant API. Область: `assistant.rs`, `assistant/{store,tools,model_auth,runtime}.rs`, shared settings helpers; сопоставление с фактическим pinned Codex protocol и Node HTTP. Это дополнительный review, не объявление всей фазы принятой.

## Обнаруженные проблемы

| Priority | Finding | Состояние на момент записи |
| --- | --- | --- |
| P1 | Message preflight status и create-run transaction разделены; Codex logout/relogin мог сменить generation между ними. Если cleanup RPC потерян, Node принимал старый run/auth и начинал inference до последующего отказа tools/finish. | Root добавил org lock первым и повторную проверку authenticated/confirmed_generation перед run creation; partial/completion также перепроверяют generation. Source просмотрен; targeted race regression выполняет отдельный verifier. |
| P2 | User/assistant messages создаются в одной transaction с одинаковым now(); сортировка `(created_at,UUIDv5)` иногда ставила ответ перед вопросом в UI/history. | Root добавил persisted sequence и ordered_messages с fallback user-before-assistant. Source просмотрен, runtime/model history mapping исправлен. |
| P2 | Completed NDJSON model/usage отбрасывались; Run metadata оставалась null. | Root сохраняет bounded model и numeric usage projection. Source просмотрен. |
| P2 | Gateway validate выполнялся до authority/receipt/count; schema errors не расходовали durable tool budget и changed-args invalid shape обходил replay409. | Root перенёс validation внутрь savepoint после authority/receipt/count, включая read-only provider-test preparation. Unknown name в audit нормализуется. Source просмотрен. |
| P2 | Некоторые conversation CRUD брали convo lock, затем INSERT audit/credential требовал org FK KEY SHARE; параллельный MCP держал org FOR UPDATE и ожидал convo: inversion/deadlock. | Root исправил settings lock на FOR NO KEY UPDATE и добавил org-first в store::conversation/finish/credential maintenance. Повторный read-only source review подтвердил единый порядок. Actual concurrency regression остаётся отдельной проверкой verifier. |

## Подтверждённые границы в source

Auth completion повторно проверяет hash/original session/owner/org, pending/approved attempt, TTL и generation под locks; положительный ответ содержит approved=true/exact generation. Pending device code projection доступна только исходной session/user. Status с одним старым Node auth file не включает DB auth после restore. APIkey и credential ciphertext не входят в public DTO/model history; credential handle consumed атомарно с encrypted connection service. Чужой user/org/conversation не может прочитать handle или chat.

Tool authority выводится из opaque capability и живого owner membership/session, не из аргументов модели. Mutations используют общий revision lock/services/audit и persisted receipt; replay возвращает прежний результат, semantic failure откатывается savepoint-ом при сохранённом receipt/budget. Credential TTL maintenance физически удаляет ciphertext/документ, не ограничивается отказом lookup. Runtime не считает EOF completed; rechecks original authority при partial/completion, cancellation не обещает rollback committed settings.

Node сторона проверена15tests локально и в настоящем hardened Docker. Реальный pinned Codex0.167.2 с тем же capability config и loopback Responses fixture вызвал разрешённый SDK MCP tool и отклонил exec_command; filesystem marker отсутствует, native environment caps0, external model requests0. Machine report `data/eval/assistant-runner-smoke.json`. Указанные tests не являются live model/device/Telphin acceptance.

## Осталось

Root API integration/PG tests, browser owner-chat/secure-card/mobile flows и исправления review проверяются отдельными исполнителями. Этот срез не заявляет их своим результатом. Реальные ChatGPT device approval/APIkey entitlement, модельное качество и подключение разрешённого Телфина остаются отдельными live gates. Пользовательский UAT не получен.

## Повторный affected review

После root fixes все первоначальные finding имеют статус **source-resolved**: generation повторно проверяется под общим org lock до создания/частичного сохранения/завершения run; sequence задаёт порядок сообщений; model/usage сохраняются bounded projections; failure validation находится внутри savepoint после receipt/authority/count; lock order согласован и org lock совместим с FK KEY SHARE. Новых P1 в проверенном scope не выявлено. Actual PG/race/browser integration не приписывается этому source-review.

Prompt runner явно разрешает connect_telphin по credential_ref, запрещает IAM, считает последнее role=user текущей задачей и отличает прошлые сообщения/tool results от новых команд. Fixed8 catalog с AST/criterion/working-hours shape совместим с SDK validator. В native-каталоге нет OS exec/read/edit/web/agent capabilities; resource helper functions относятся только к одному server без resource handlers.

Дополнительный P2 nested schema parity также **source-resolved**: root добавил strict nested keys/types для rubric criteria и working_hours, ограничение criterion ID до100символов; shared validators проверяют остальные обязательные поля и значения. Повторное чтение `tools::validate` подтвердило соответствие fixed8 schema и server-authoritative проверок. После этих исправлений открытых P1/P2 в данном read-only scope не найдено. Это заключение о source; реальные PG/browser acceptance и live gates учитываются отдельными отчётами.

## Основной API ↔ реальный Node: noauth wire smoke

После финального deployment выполнен HTTP smoke основного API18080 с настоящим private Node runner: `runner_ready=true`, Codex `0.167.2`, `authenticated=false`. Noauth message отклонён409 `assistant_auth_required`; повторный GET подтвердил отсутствие messages/runs. Случайная capability отклонена403. Созданный синтетический чат удалён, последующий GET404; тестовая HTTP session завершена. Настройки компании и Codex auth не менялись. Внешних auth/model/paid inference запросов0. Агрегированный отчёт без ID/PII: `data/eval/assistant-main-wire-smoke.json`. Это проверка wire/отказа без авторизации; live авторизацию и качество модели она не подтверждает.
