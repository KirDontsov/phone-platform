# Implementation contract v1

REST base `/api`; successful responses `{ "data": ... }`; errors `{ "error": { "code": "...", "message": "Russian message" } }`. JSON only except media. Cookie session, credentials include. Authenticated writes require `X-CSRF-Token` returned by me/login; optional `X-Organization-Id` must equal active membership. No auth token in localStorage. All returned datetimes ISO UTC, IDs UUID strings, unknown/null explicitly preserved.

## Auth and memberships

- POST /auth/login `{email,password}` -> Me
- GET /auth/me -> Me
- POST /auth/logout `{}` -> `{ok:true}`
- POST /auth/organization `{organization_id}` -> Me (rotate CSRF; clear stores/cancel requests)
- POST /auth/reset/request `{email}` -> `{ok:true}` (mail catcher; never echo token)
- POST /auth/reset/confirm `{token,password}` -> `{ok:true}`
- POST /auth/invitations/accept `{token,password,name}` -> Me
- GET /members -> `{items:[{id,email,name,role,extension}],invitations:[{id,email,role,expires_at,revoked}]}`
- POST /members/invite `{email,role}`; PATCH /members/:id `{role,extension}`; DELETE /members/:id; DELETE /members/invitations/:id

Me = `{user:{id,email,name},organization:{id,name,timezone,callback_minutes,retention_days},organizations:[{id,name,role}],role,csrf_token,demo_mode:boolean}`.

## Calls and dashboard

- GET /calls?limit=25&cursor=...&search=...&from=...&to=...&filter=<JSON AST> -> `{items:Call[],total,next_cursor,coverage:{complete:boolean,reason:string}}`
- GET /calls/:id -> `{call:Call,legs:[],recordings:[],transcripts:[],analyses:[],history:Call[],audit:[],external_links:[]}`
- PATCH /calls/:id `{status_id?,tag_ids?,note?,contact_kind?,manual_lock?:boolean}` -> Call; null clears nullable value; optimistic version in `{version}` if supplied
- DELETE /calls/:id -> `{ok:true}` (enqueue purge, immediately inaccessible)
- POST /calls/:id/reprocess `{stage:'transcribe'|'analyze'|'all'}` -> `{job_id}`
- POST /calls/:id/transcripts/:tid/roles `{roles:{speakerId:"manager"|"client"|"unknown"}}` -> `{transcript,job_id}`; immutable new transcript version, manager/owner only
- GET /calls/:id/media/:recording_id -> authorized Range audio; missing record ->404 JSON
- GET /dashboard?from=&to=&filter= -> `{totals:{calls,answered,missed,unhandled,overdue,analyzed,average_score},daily:[{date,calls,missed}],employees:[{name,calls,average_score}],scores:[{bucket,count}],coverage:{complete,reason}}`
- GET /callbacks -> `{items:[{call_id,phone,employee,opened_at,due_at,attempts,state:'open'|'overdue'|'resolved'|'uncertain',reason}],as_of}`

Call = `{id,provider_id,started_at,phone,contact_name,employee,direction:'in'|'out'|'internal',provider_result:'busy'|'answered'|'bridged'|'not answered'|'rejected'|'failed'|'unknown',human_connected:boolean,duration:number,stage:'imported'|'recording_pending'|'transcribing'|'analyzing'|'ready'|'partial'|'failed'|'skipped',status_id:string|null,tag_ids:string[],contact_kind:'primary'|'repeat'|'unknown',score:number|null,has_recording:boolean,recording_state:'ready'|'pending'|'rate_limited'|'failed'|'none'|'unknown',note:string,manual_lock:boolean,version:number,coverage_complete:boolean,deleted?:boolean}`.

`recording_state` is computed from organization-scoped recording documents, independently of processing `stage` and provider `has_recording`. Any ready recording wins; otherwise priority is rate_limited, pending, failed. Without recording documents, explicit `has_recording=false` means none; missing/null/true means unknown. A stale rate-limit error on a failed recording remains failed. Summary exposes no media paths or provider URLs. `ready` means a recording has completed download; actual media access still requires authorization.

Transcript = `{id,version,provider,state,language,role_confidence:'known'|'uncertain',segments:[{id,speaker,role:'manager'|'client'|'unknown',start,end,text}],created_at}`.
Analysis = `{id,state,model,provider,prompt_version,rubric_version,transcript_id,summary,customer_intent,agreements:[],next_steps:[],suggested_status_id,suggested_tag_ids:[],criteria:[{id,name,weight,score:number|null,evaluation_mode:'score'|'checklist',fulfilled:boolean|null,status:'scored'|'not_applicable'|'insufficient_evidence',evidence:[{segment_id,start,end,quote}],reason}],score:number|null,abstention_reason,created_at,usage:{input_tokens,output_tokens,cost}}`.

## Configuration, filters and providers

- GET /taxonomy -> `{tags:[{id,name,color,archived}],statuses:[{id,name,color,archived}]}`
- POST /taxonomy `{kind:'tag'|'status',name,color}`; PATCH /taxonomy/:id `{name?,color?,archived?}` -> taxonomy item
- GET /rubrics -> `{items:[{id,name,version,active,criteria:[{id,name,weight,required,instruction,evaluation_mode?:'score'|'checklist'}]}]}`; POST /rubrics `{name,criteria}` creates new immutable version; POST /rubrics/:id/activate. Missing evaluation_mode defaults to score. Checklist scored criteria require an explicit fulfilled boolean with score5/0; unscored criteria require fulfilled=null and score=null. Server validates evidence and derives aggregate score; unknown speaker roles suppress checklist verdicts. Legacy score criteria never acquire an inferred binary verdict
- GET /filters -> `{items:[{id,name,ast}]}`; POST /filters `{name,ast}`; DELETE /filters/:id
- AST group `{op:'and'|'or',rules:[AST...]}`, `{op:'not',rule:AST}`, predicate `{field,op,value}`. Fields: direction,provider_result,employee,status_id,tag_ids,contact_kind,stage,score,duration,started_at,has_recording,recording_state,human_connected,callback_state,callback_age_minutes,callback_attempts,analysis_state,transcript_state. Ops: eq,ne,in,contains,gt,gte,lt,lte,is_null; type validated server-side. Max depth6,64 nodes. Empty `and` matches all.
- GET /connections -> `{items:[{id,kind:'telphin'|'amocrm'|'sheets'|'asr'|'analysis',state:'configured'|'disconnected'|'error',config:<redacted settings>,last_error,last_sync_at}]}`
- PUT /connections/:kind `{config:{...}}` owner-only; secrets write-only; blank/absent secret retains previous; explicit disconnect via DELETE /connections/:kind
- POST /connections/:kind/test -> `{ok,state,message}`
- POST /connections/telphin/sync `{from?,to?,all_available?}` -> `{job_id}`
- PATCH /organization `{name?,timezone?,callback_minutes?,working_hours?,retention_days?,callback_allow_inbound?,callback_min_duration?}` -> organization

Telphin input config `{host,client_id?,app_id,app_secret}`. Missing/blank client_id is resolved server-side via authenticated /user/ before saving; persisted config always contains a validated numeric client_id. Failed lookup stores no connection and emits a safe error. ASR `{provider:'openai'|'http'|'demo',model,base_url,api_key}`. Analysis `{provider:'opencode'|'openai'|'http'|'demo',model,base_url,api_key,budget_daily,timeout_seconds}`. Default demo providers only in explicit DEMO_MODE; production reports missing configuration and never fabricates transcript/analysis. Secrets redacted as `<configured>` in GET.
amoCRM config `{account_domain,client_id,client_secret,redirect_uri}`; Sheets `{client_id,client_secret,redirect_uri,spreadsheet_id,sheet_name,column_mapping}`.

## Integrations and operations

- GET /integrations -> `{links:[],deliveries:[]}`; POST /integrations/link `{call_id,provider:'amocrm',contact_id,deal_id}`; POST /integrations/export `{call_ids:[],provider:'amocrm'|'sheets'}` -> jobs
- GET /integrations/:provider/authorize -> `{url}`; GET /integrations/:provider/callback?code=&state= (OAuth redirect)
- GET /integrations/amocrm/search?phone= -> `{contacts:[],deals:[]}` (owner/manager; ambiguity explicit)
- GET /operations -> `{jobs:[],sync_runs:[],deliveries:[],usage:{},health:{},audit:[]}`
- POST /operations/jobs/:id/retry; POST /operations/purge; GET /operations/audit -> `{items:[]}`
- GET /health live; GET /ready actual dependencies.

RBAC owner all; manager call annotations/reprocess/filters/links/exports/read; reviewer read only. Changing provider secrets/rubric/taxonomy/members/organization is owner-only. All lookups scoped to session organization. Demo credentials delivered via local setup instructions, never copied from Bondscan. Demo includes two organizations and isolated rich synthetic calls.

## Уточнения финального ревью

- PATCH разметки допускает сохранение уже назначенного архивного status/tag, запрещая новое назначение архивных значений. Частичный note-only PATCH перед включением manual_lock сохраняет вычисленный тип обращения.
- POST `/api/calls/{call_id}/transcripts/{transcript_id}/roles`: `{roles:{speakerId:"manager"|"client"|"unknown"}}`. Допустимы только существующие speaker IDs. Создаётся immutable версия расшифровки, исходная остаётся доступна; результат `{transcript,job_id}`. Повтор одинакового подтверждения идемпотентен, создание версий сериализуется блокировкой звонка.
- Версии rubric создаются и активируются под блокировкой организации; одновременно активна одна шкала. Изменение шкалы не переписывает старые анализы.
- `operations.usage.cost` — null при хотя бы одном запуске с неизвестной фактической стоимостью; `known_cost` — сумма известных цен, `unknown_cost_runs` — число остальных. Нулевую цену нельзя выводить вместо неизвестной.
- OAuth `expires_at` хранится как integer epoch seconds, согласованно API/worker. Retention job получает фактический `organization.retention_days`.
- Приватный runner `http://model-runner:4096` может получить server password из runtime оператора при сохранении analysis provider=opencode без ключа. Для произвольных URL этот секрет не подставляется; секрет не возвращается клиенту.

## Обработка звонков и относительные фильтры (фаза13)

Call дополнительно содержит `transcript_state:'ready'|'missing'`, `analysis_state:'analyzed'|'partial'|'not_analyzed'`, `processing:{state:'idle'|'queued'|'running'|'retry'|'failed'|'completed'|'skipped',step:'transcribe'|'analyze'|'reprocess'|null,job_id:string|null,error_code:string|null,queued_at:string|null,updated_at:string|null,next_attempt_at:string|null,attempts:number,max_attempts:number}`. Готовность анализа определяется документом analysis для последней версии transcript. Настоящий результат с null score или abstention имеет analysis_state=partial; прежний анализ остаётся в истории. Call.score и dashboard averages используют только актуальный результат, отсутствие анализа не считается нулём. totals.analyzed включает analyzed и partial.

GET `/calls/:id` дополнительно возвращает `processing:{...Call.processing,queue:{scope:'organization',ready_ahead:number|null,ready_total:number,running_total:number,scheduled_total:number,estimated_wait_seconds:null},providers:{asr:{ready:boolean,reason:string|null},analysis:{ready:boolean,reason:string|null}}}`. Queue учитывает все активные задания собственной компании, не ограничивается последними200 событиями. ready_ahead — число готовых к запуску задач компании перед выбранным заданием, а не позиция в общей очереди; для выполняемого/отложенного/терминального задания null. ETA неизвестен. Поля payload, lease_owner, конфигурация провайдера и произвольный текст ошибки не выдаются. providers.ready означает наличие подходящей конфигурации, не live-проверку внешнего сервиса; demo отклоняется для реальных звонков. Неизвестная legacy ошибка преобразуется в processing_failed.

POST `/calls/:id/reprocess`: новый запрос200 `{job_id}`, уже активное задание202 `{job_id,reused:true}`. Проверка и enqueue сериализованы tenant/call row lock; активный wrapper/child возвращается до проверки изменившейся конфигурации. Новый transcribe требует готовую запись и ASR; analyze требует готовую расшифровку, analysis provider и активную rubric; all требует запись, оба провайдера и rubric и создаёт новую расшифровку. Неподходящие prerequisites422, без ложного enqueue. Transcribe сохраняет текст даже если последующий анализ ещё не настроен. Worker независимо проверяет актуальные настройки при выполнении.

AST дополнительно допускает text поля analysis_state/transcript_state и numeric поля callback_age_minutes/callback_attempts. Последние вычисляются для первого пропущенного звонка эпизода до date/search фильтров; на остальных звонках null. callback_age_minutes — прошедшие календарные минуты с opened_at, callback_attempts — исходящие попытки связи. Рабочие часы влияют на due_at, но не на возраст. Подтверждённые неперезвоненные: callback_state in [open,overdue] AND callback_age_minutes gte X*60; дополнительное callback_attempts eq0 исключает попытки без ответа. uncertain выбирается отдельно. GET `/callbacks?filter=<AST>&from=&to=&search=` использует тот же фильтр звонков, items дополнительно содержат callback_age_minutes/callback_attempts; состоявшийся перезвон вне выбранного периода всё равно закрывает эпизод.

Новая выгрузка в amoCRM/Sheets использует analysis последней расшифровки и его score; устаревший сохранённый Call.score не экспортируется. После первой попытки доставки analysis_id фиксируется, включая null, и не меняется при retry/reconciliation. Недоступный зафиксированный анализ даёт pinned_analysis_missing вместо подмены другим результатом.
