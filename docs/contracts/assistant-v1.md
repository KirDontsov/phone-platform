# Assistant v1 — контракт фазы 11

Дата: 2026-11-12. Чат владельца настраивает свою компанию обычными сообщениями. Отдельный Docker Codex assistant, отдельный настоящий stdio MCP bridge; существующий analysis runner остаётся без MCP. Bondscan используется только как read-only образец UI/потока. Никакие его данные/секреты не используются.

## Власть и секреты

Все public endpoints `/api/assistant` используют существующую cookie-сессию, проверенное membership/owner, CSRF/Origin для writes. Чаты принадлежат `(organization_id,user_id)`; другой пользователь даже той же компании не получает чужую историю/доступы. MCP opaque capability ограничен оригинальной живой сессией, owner, компанией, conversation/run, expiry и состоянием run. Logout/switch/removal/cancel/delete запрещают последующие tool calls. Компания, пользователь и session не являются аргументами инструмента.

Доступы передаются secure card внутри чата, сохраняются AES-GCM runtime key и не входят в chat text, model prompt, events, audit или tool results. Модель получает `credential_ref`. Обычный текст не является секретным каналом; очевидные вставки `app_secret`, API keys/private keys/password assignments отклоняются с предложением secure card без сохранения/эха. Нельзя обещать regex-распознавание любого opaque секрета.

Разрешены только перечисленные конфигоперации. Нет IAM/приглашений/удаления звонков/retention/export/arbitraryHTTP/SQL/shell/edit/repo/Docker socket. Подтверждение verified history boundary не делегируется модели. Текст звонков не передаётся настраивающему агенту. Device/APIkey login Codex — отдельные owner control endpoints, не MCP и не prompt.

## Public REST

Envelope/error прежние `{data:...}` / `{error:{code,message}}`. UUID IDs, UTC RFC3339 times. Все DTO — явные projections, не generic document_json для приватных записей.

- `GET /api/assistant/status`: `{available:boolean,runner_ready:boolean,authenticated:boolean,auth_mode:null|"apiKey"|"chatgpt",codex_version:string|null,model:string|null,error_code:string|null,pending_login:null|{login_id,verification_url,user_code,expires_at}}`. Недоступность runner/auth видна, fake reply отсутствует.
- `POST /api/assistant/auth/start`: `{type:"chatgptDeviceCode"}` или `{type:"apiKey",api_key:string}`; результат status с pending_login либо authenticated. APIkey только secure form; не messages/history.
- `POST /api/assistant/auth/cancel`: `{login_id:string}` → status. `POST /api/assistant/auth/logout`: `{}` → status.
- `GET /api/assistant/conversations?limit=30&cursor=...`: `{items:Conversation[],next_cursor:string|null}`. Cursor ограничен owner/org. Только undeleted свои.
- `POST /api/assistant/conversations`: `{title?:string,client_request_id:UUID}` → `Conversation`. Повтор ключа/того же тела возвращает тот же результат, изменённое тело409.
- `GET /api/assistant/conversations/:id`: `{conversation:Conversation,messages:Message[],runs:Run[],credentials:CredentialSummary[]}`. Ограниченная история; максимум500messages,40последнихmessages/128KiB в модели. Смена/утрата контекста сервером проверяется независимо от frontend.
- `PATCH /api/assistant/conversations/:id`: `{title:string,version:number}` → Conversation (optimistic409).
- `DELETE /api/assistant/conversations/:id`: revoke/cancel run, remove encrypted credentials/history; `{ok:true}`. Audit сохраняет только action/IDs.
- `POST /api/assistant/conversations/:id/credentials`: `{kind:"telphin",label?:string,config:{host?:"apiproxy.telphin.ru"|"altapi.telphin.ru",app_id:string,app_secret:string,client_id?:string}}` → CredentialSummary. Можно вставить JSON в эту форму, не в обычное сообщение. Если client_id отсутствует или пуст, сервер получает токен приложения и определяет числовой ID АТС через /api/ver1.0/user/ до сохранения handle; пользователь вручную его не ищет. Scope добавочного/чужой явный ID не принимаются. TTL24h для неприменённого handle, после подключения секрет хранится существующим encrypted connection service; stale/foreign refs отвергаются.
- `DELETE /api/assistant/conversations/:id/credentials/:credential_id`: `{ok:true}`; неприменённый handle уничтожается. Подключение Телфина этим не отключается.
- `POST /api/assistant/conversations/:id/messages`: `{text:string,client_request_id:UUID,credential_refs?:UUID[]}` → `{run:Run,user_message:Message,assistant_message:Message}` HTTP202. 1 active run на conversation; повтор ключа возвращает тот же run, изменение тела409. Нет auth → видимая ошибка409 `assistant_auth_required`, сообщение с ключом не сохраняется. Текст1..8000chars, максимум8credential_refs. Credentials своей conversation+user+org. Текст/credential refs после нормализации входят в hash для idempotency.
- `GET /api/assistant/runs/:id`: `{run:Run,messages:Message[]}`. Для активного run frontend polling1s (это текущий client transport, не обещание SSE); runner stream читает backend, partial assistant.text и durable events доступны через polling. Терминальность определяется run.state, не EOF.
- `POST /api/assistant/runs/:id/cancel`: `{}` → `{run:Run}`. Сначала revoke capability, затем interrupt Codex. Уже committed actions остаются; receipt/result позволяют увидеть фактические изменения. После cancel нельзя обещать rollback.

Conversation `{id,title,version,active_run_id:null|UUID,created_at,updated_at}`.
Message `{id,role:"user"|"assistant",text,status:"ready"|"running"|"completed"|"failed"|"cancelled",run_id:null|UUID,created_at}`.
Run `{id,conversation_id,state:"queued"|"running"|"completed"|"failed"|"cancelled"|"interrupted",assistant_message_id,events:Event[],error_code:null|string,model:null|string,usage:object|null,started_at,finished_at:null|time}`.
Event `{seq:number,type:"status"|"tool",tool_name?:string,title:string,status:"running"|"completed"|"failed"|"uncertain",summary?:string}`. Никогда raw args/tool outputs/secret blobs.
CredentialSummary `{id,kind:"telphin",label,created_at,expires_at}`. Нет config/token/secret/masked-value substitution.

## MCP bridge → API

Настоящий stdio MCP server использует официальный `@modelcontextprotocol/sdk`; Codex подключает только этот фиксированный server. Bridge отправляет Bearer capability в `POST /api/assistant/tools`:

`{method:"list"}` → `{data:{tools:[{name,description,inputSchema}]}}`.
`{method:"call",call_id:string,name:string,arguments:object}` → `{data:{result:object}}`.

Это внутренний HTTP gateway, не выдаваемый за MCP transport; MCP transport — stdio SDK. API и bridge принимают только schema-validated allowlist. Unknown tool/extra fields/foreign credential/version conflict → bounded safe tool error. Никаких generic call_endpoint/config_sql.

Tools:

1. `get_workspace_configuration {}` — `{revision:integer,...}` и текущие taxonomy/filter/rubric/callback settings, masked Telphin state и безопасный sync progress. Без calls/transcripts/audio/auth config.
2. `connect_telphin {credential_ref:UUID,expected_revision:integer}` — сохранить подключение через существующий общий connection service, безопасный redacted результат.
3. `test_telphin_connection {}` — реальная ограниченная проверка доступа своим provider helper; без выводов «история полна». Read-only внешний запрос с timeout.
4. `sync_telphin {from?:ISO,to?:ISO,all_available?:boolean,expected_revision:integer}` — durable org-scoped job. `history_start_verified` не аргумент. Без owner-verified boundary coverage остаётся неизвестным.
5. `upsert_taxonomy {expected_revision:integer,kind:"tag"|"status",name:string,color?:hex,id?:UUID,version?:integer,archived?:boolean}` — по id/version обновление; без id точно одно совпадение по имени повторно используется, неоднозначность — ошибка, новое создаётся с deterministic tool-call UUID. Общие валидаторы/API services и audit.
6. `upsert_filter {expected_revision:integer,name:string,ast:object,id?:UUID,version?:integer}` — общий typed bounded AST; name lookup/id/version как выше. Нет SQL от модели.
7. `save_rubric {expected_revision:integer,name:string,criteria:array,activate?:boolean}` — immutable version и серверные критерии/веса/правила; одинаковое содержимое повторно используется; активация отдельная от старых анализов. Общий service.
8. `set_callback_rules {expected_revision:integer,timezone?:string,callback_minutes?:integer,working_hours?:object|null,callback_allow_inbound?:boolean,callback_min_duration?:integer}` — merged domain policy validation. Нет retention/IAM/name/user/model budget.

Каждый mutation — единая PostgreSQL transaction: определить scope по opaque token → общий lock организации → lock conversation/run → повторно проверить capability и живые session/membership под locks (включая блокировку от concurrent logout/role removal) → receipt `(run,call_id,tool,args_hash)` → общий domain service + audit → safe result + done receipt. Повтор того же call_id возвращает persisted result; другое имя/args409; pending не повторяет side effect. Read-only provider test может исполняться вне SQL transaction, его status durable и повтор не выполняет write. Все mutations требуют `expected_revision` (неотрицательный integer) из workspace snapshot или последнего успешного результата. Общие HTTP/API services повышают `settings_revision` при изменениях settings; MCP проверяет её под org lock и возвращает новую revision. Для обновления конкретного id дополнительно обязателен `version`; отсутствие/устаревание —409, никакого last-write-wins. Cancel использует те же conversation/run locks: уже committed action сохраняется, все действия после точки отзыва запрещены.

Tool budget24/run, run deadline180s, concurrency2/process +1/conversation; лимиты сообщений/ответов/списков обязательны. Timeout/restart никогда не считается completed без terminal event. Run lease/expiry позволяет после API crash пометить abandoned run interrupted и отозвать capability, не зависнуть навсегда.

## Runner HTTP (private service)

Отдельный `assistant-runner:8091`, без host ports; Bearer `ASSISTANT_RUNNER_TOKEN` во всех управляющих вызовах. API берёт URL/token только из runtime оператора. `/health` без user data, доступен Docker healthcheck.

- `POST /auth/status {organization_id:UUID}` → `{authenticated,auth_mode,codex_version,model,pending_login,error_code}`.
- `POST /auth/start {organization_id,auth_attempt_id:UUID,generation:integer,authorization_token:string,type:"apiKey"|"chatgptDeviceCode",api_key?:string}` → такой же status. Device flow возвращает login_id/verification_url/user_code/expires_at. URL только официального auth origin, коды не идут в модель/audit. Нет успешного model access по одному model/list.
- `POST /auth/cancel {organization_id,login_id,generation:integer}`; `POST /auth/logout {organization_id,generation:integer}`.
- `POST /turn {organization_id:UUID,auth_generation:integer,run_id:UUID,mcp_token:string,history:[{role:"user"|"assistant",text:string}],credential_refs:CredentialSummary[]}` → NDJSON lines `{type:"status",message:string}` / `{type:"delta",text:string}` / `{type:"completed",text:string,model?:string,usage?:object}` / `{type:"error",code:string}`. Ровно один terminal result; invalid/oversized/unknown events fail closed. Raw Codex events/arguments/stderr не проксируются в продукт.
- `POST /cancel {organization_id,run_id}` → `{cancelled:true}`.

Pinned Codex0.167.2 / MCP SDK1.32.2, actual protocol schemas проверяются по этой версии. Auth home — private runtime per organization (не host auth); каждый turn новый ephemeral Codex thread/process/workdir с фиксированными инструкциями/конфигом и одним stdio MCP server. DB хранит историю приложения и передаёт bounded context заново. Auth flows сериализованы per org, никакого общего mutable auth/thread между компаниями. Удаление/истечение login/turn очищает subprocess/temp dirs; сигналы и HTTP cancellation убивают process group. Домашний каталог/БД/репозиторий/телефонные ключи/Docker socket в контейнер не монтируются. Код модели не выбирает RPC method/command/MCP URL.

### Завершение авторизации и отзыв

API создаёт `assistant_auth_attempt` с original user/session/org, expiry и auth generation. Authorization token хранится только hashed; Node получает raw только runtime. Перед promotion staged auth Node вызывает `POST /api/assistant/auth/complete` с Bearer authorization_token и `{auth_attempt_id,generation}`. API атомарно проверяет живые original session/membership/owner, актуальную generation, pending state и TTL, фиксирует разрешение/audit или отказывает. Node никогда не переносит staged auth в canonical home после отказа. Эта transaction — точка авторизации подключения: успешное подключение до обычного logout остаётся настройкой компании; завершение после уже отозванной сессии запрещено.

Новая попытка/cancel/Codex logout повышают generation, старый callback не завершает новую попытку. Node тоже сериализует generation и promotion/delete под одним org mutex. Turn/status используют только подтверждённую API generation; stale credentials недоступны даже при оборванном cleanup RPC и удаляются при recovery. На status pending login code видит только инициировавшая оригинальная сессия; другой owner может отменить попытку, но не получает код. Обычный logout/switch отзывает pending attempt через session check; активное ранее авторизованное подключение компании не является browser session.

### Хранение, удаление и восстановление

Каталоги auth/temp имеют0700, файлы0600. Неприменённый credential handle живёт24h; после успешного connect исходный ciphertext уничтожается, безопасная metadata не даёт повторного доступа. Не реже раза в минуту maintenance физически удаляет expired ciphertext и auth-attempt secrets, отзывает expired runs, очищает прекратившиеся subprocess/temp artifacts. Сбой запроса не делает lookup denial единственной очисткой. Удаление conversation/credential записывает tenant tombstone, чтобы старый backup не воскресил удалённый текст или доступы.

Auth volume Codex не входит в обычный backup; после restore нужна новая авторизация. Restore отзывает sessions, assistant capabilities/pending auth attempts, прерывает восстановленные active runs, уничтожает credential ciphertext и сбрасывает подтверждённое assistant auth state. Присутствие старого auth.json в runtime cache не делает его активным: API подтверждённая generation обязательна перед каждым turn/status. Deletion ledger replay расширяется на assistant conversation/credential UUID и каскадных потомков; provider_id matching остаётся только у calls. Проверки восстановления подтверждают отсутствие deleted chat/secret resurrection и отказ старых capabilities. Внешние provider credentials подключения живут по прежнему encrypted connection контракту, handle не является вторым постоянным хранилищем.

## Проверка

Actual pinned CLI initialize/schema/auth-unconfigured/config guard + forced forbidden tool calls через локальный Responses fixture (реальный Codex, не fake app-server): отсутствие shell/exec/read/edit/web/agents в upstream tool catalog и unsupported exec без создания marker; плюс real MCP SDK tools/list/call на временной компании; без платных inference и чужого auth. Rust/PG: scope/role/session-switch/revoke, secrets absent from projections, idempotency/changed args409, archived IDs/CAS/AST, cancel/late tool denial, stale run recovery. Node: mocked Codex wire transcripts для stream/auth/cancel/errors + реальные CLI/SDK no-inference smoke; mock — только тест, не runtime fallback. Browser: owner chat/fullpage/mobile/history, credentials secure form, company switch late response, visible missing auth, run tool activity/stop. Live Codex model + real Telphin после настройки владельцем помечаются отдельным pending acceptance, не выдаются за synthetic PASS.
