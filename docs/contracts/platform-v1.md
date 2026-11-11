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

Call = `{id,provider_id,started_at,phone,contact_name,employee,direction:'in'|'out'|'internal',provider_result:'busy'|'answered'|'bridged'|'not answered'|'rejected'|'failed'|'unknown',human_connected:boolean,duration:number,stage:'imported'|'recording_pending'|'transcribing'|'analyzing'|'ready'|'partial'|'failed'|'skipped',status_id:string|null,tag_ids:string[],contact_kind:'primary'|'repeat'|'unknown',score:number|null,has_recording:boolean,note:string,manual_lock:boolean,version:number,coverage_complete:boolean,deleted?:boolean}`.

Transcript = `{id,version,provider,state,language,role_confidence:'known'|'uncertain',segments:[{id,speaker,role:'manager'|'client'|'unknown',start,end,text}],created_at}`.
Analysis = `{id,state,model,provider,prompt_version,rubric_version,transcript_id,summary,customer_intent,agreements:[],next_steps:[],suggested_status_id,suggested_tag_ids:[],criteria:[{id,name,weight,score:number|null,status:'scored'|'not_applicable'|'insufficient_evidence',evidence:[{segment_id,start,end,quote}],reason}],score:number|null,abstention_reason,created_at,usage:{input_tokens,output_tokens,cost}}`.

## Configuration, filters and providers

- GET /taxonomy -> `{tags:[{id,name,color,archived}],statuses:[{id,name,color,archived}]}`
- POST /taxonomy `{kind:'tag'|'status',name,color}`; PATCH /taxonomy/:id `{name?,color?,archived?}` -> taxonomy item
- GET /rubrics -> `{items:[{id,name,version,active,criteria:[{id,name,weight,required,instruction}]}]}`; POST /rubrics `{name,criteria}` creates new immutable version; POST /rubrics/:id/activate
- GET /filters -> `{items:[{id,name,ast}]}`; POST /filters `{name,ast}`; DELETE /filters/:id
- AST group `{op:'and'|'or',rules:[AST...]}`, `{op:'not',rule:AST}`, predicate `{field,op,value}`. Fields: direction,provider_result,employee,status_id,tag_ids,contact_kind,stage,score,duration,started_at,has_recording,human_connected,callback_state. Ops: eq,ne,in,contains,gt,gte,lt,lte,is_null; type validated server-side. Max depth6,64 nodes. Empty `and` matches all.
- GET /connections -> `{items:[{id,kind:'telphin'|'amocrm'|'sheets'|'asr'|'analysis',state:'configured'|'disconnected'|'error',config:<redacted settings>,last_error,last_sync_at}]}`
- PUT /connections/:kind `{config:{...}}` owner-only; secrets write-only; blank/absent secret retains previous; explicit disconnect via DELETE /connections/:kind
- POST /connections/:kind/test -> `{ok,state,message}`
- POST /connections/telphin/sync `{from?,to?,all_available?}` -> `{job_id}`
- PATCH /organization `{name?,timezone?,callback_minutes?,working_hours?,retention_days?,callback_allow_inbound?,callback_min_duration?}` -> organization

Telphin config `{host,client_id,app_id,app_secret}`. ASR `{provider:'openai'|'http'|'demo',model,base_url,api_key}`. Analysis `{provider:'opencode'|'openai'|'http'|'demo',model,base_url,api_key,budget_daily,timeout_seconds}`. Default demo providers only in explicit DEMO_MODE; production reports missing configuration and never fabricates transcript/analysis. Secrets redacted as `<configured>` in GET.
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
