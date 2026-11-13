---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: Phone Platform
status: phase_20_r1_recovery_release_pending
stopped_at: Phase20 callback result and Telphin cooldown queue fixes tested; release pending; real RouterAI consent pending; phase12 paused
last_updated: "2026-11-13"
progress:
  total_phases: 20
  implemented_phases: 18
  locally_verified_phases: 18
  accepted_phases: 0
  completed_phases: 0
  total_plans: 38
  implemented_plans: 36
  completed_plans: 1
---

# Project State

## Текущее состояние — фаза20

Release verifier обнаружил конфликт: прежний default90days retention по возрасту звонка под новым приоритетом очистил120calls и скрыл24528. Ownworker остановлен до R1. Приватный manifest и дополнительный PGsnapshot сохранены; guarded bulk repair вернул24528и отменил24528retention-onlypurges, manual/tombstonesexcluded. R1 explicitretention_enabled(defaultfalse) + livepurgeguard/API/UI implemented; API2/worker63/UI100/typebuild и batchactualPG PASS, ключевыеRED подтверждены. 120реальныеsourcemetadata/audio recovery CLI проходит отдельный gate; источник/восстановление ещёpending, не объявлять данные полными.

RouterAI пока не подключён: собственный helper истёк, ключ в runtime не сохранён; причина пустой страницы у провайдера не доказана. Исправлена собственная callback-страница: понятные ошибки, чистый URL, terminal expiry, ожидание600с/результат60с. Python14/14, новые3+timeout RED, настоящий Chrome synthetic callback/metaRefresh/mobile PASS. Телфин: 5187 ready файлов на месте, 28746 recording ждут quota; найдено вытеснение sync массовыми retry. Claim теперь учитывает cooldown своей компании до попытки, даёт due sync/retention/purge приоритет; cached backoff не продлевает quota, ready/path сохраняются, успешный download очищает last_error. ActualPG/worker и UI98/type/build PASS, independent plan-check0/reviews PASS. Выпуск и verifier ожидаются; реальный consent, завершение истории и humanUAT pending. Phase12/stash сохранены.

## История реализации — фаза19

Пользователь подтвердил RouterAI. Егоpubliccatalog606 содержитобаисходныхSTTmodelID; наэтомкомпьютереестьproviderdefs, credentialsнетниinline/env/legacy/SQLite, вPhone-компанииASR/analysisconnectionsещёнесохранены. ДобавленыexplicitlocalProviderRouterAI/fixedendpoint/tenant/keyseparation, exact2ASRcatalog+OpenCodeconfig, RUBseparate/commoncostNULLвключаяnativeanalysis. UI98PASS/невыбираетплатныемоделиавтоматически. OwnPKCEhelper/команда ./start-analyzer.sh connect-routerai открываетаккаунтбезкопированияключа; current-filelockmerge/600/atomicinstallrollback, Host/state/iss/TTL/query/replay/oneexchangeпроверены.

GSDPLAN-CHECKrevision1/reviewPASS. Code df7a162/nativeimages15-df7a162 опубликованы, API/worker/clientобновлены8healthy; unchangedassistant/relayсохраненыживымиподстарымtagсодинаковымиdigests. ЛокальныйOpenCodeпересобранhealthy, SSH/pairingсохранены; Telphinretry/attempt2/next/window33page7/calls43370неизменилисьприidleworkerlease0rollout. Node30, workerlib19+HTTP13+providers16(1existingPGskip), Python29, frontend98/typebuildPASS; основныеREDпроверены, каждыйassertнеобъявленRED. Publicowner/tenant/media/Codexstatus/settingsdesktop/mobile/doctorPASS. Helperреальнозапущен, браузерRouterAIоткрыт, состояниеawaiting_consent; подтверждениеаккаунта/реальныйinference/ASRqualitypending, никакихплатныхproviderзапросов/аудиооператоромвфазе19. Детали19-SUMMARY/VERIFICATION/UAT. Другиеhuman-gates/phase12stashсохранены.

## История реализации — фаза18

Исправлен Codexremote403: authenticatedexact-hostCONNECTчерезсуществующийlocalSSH4096, explicitproxyenv/NO_PROXY; настоящийpinnedCLIdeviceflowчерезштатныйproductionserviceenvPASS. Переавторизациясохраняетстаруюidentity приошибке; pending/prepare/commit/receipt/logout и durablefailclosedbeforepromotion провереныNode/реальнымPGHTTP. IndependentPLAN-CHECKrevision1 иROOT/AUTHreviewPASSпослеfixes. Code d59a82e/runtime15-d59a82e опубликованы,8healthy, workeridlelease0передзаменой; Telphinoriginalretry/attempts/next_attempt/window33/page7/calls43370сохранены.

OpenCodeиспользуетсобственную600копиюсуществующихглобальныхproviderdefs freellmapi/apinex толькодляявнойcompany. Catalog29models/keylessanalysisUI, keylesslocalAPInexVoiceSTTexplicitopt-in, legacyOpenRouterдваIDотдельно; noautoconfig/paidSTTswitch. Tenant/sessionguardALLnativepaths+secretredaction, internalpairingнеуходитнаизменённыйendpoint, trustedorgизsession/job. РеальныйAPInexпринимаетaccountkey, ноsubscriptionallowedfalse/модельHTTP402; inferenceНЕработающий, externalquotaнеустранена. FreellmAPIendpointнедоступен, RouterAIключвконфигеотсутствует. Точныйproviderасинхронноспрошен, ответpending; чужиедоступынекопировать, ничего непокупать.

Frontend95/build, gateway26, assistant26/pinnedofflineMCP, workerlib19+HTTP11+providers16(1старыйPGskip), Python14, APIunits3+actualPGasr1/auth2PASS. ОсновныеREDвключая5UI/2actualPG/proxy/auth/crash/nativeguards/quotaподтверждены; Rust402отдельноREDнет. Publicowner/tenant/audio/Codexstatus/registration/browser+realanalyzerdoctorPASS; realdevicechallengePASS, liveowncatalog29/foreign403/redacted/402PASSnegative. НезависимаяVERIFICATIONв18-VERIFICATION; humanOAuth/реальныйдиалог/ASRqualitypending. OwnPGfixture/SSHforwardудалены, phase12/stash сохранены. Всеучётныезаписи/SMTP/данные/secretsосталисьпрежними.

## История реализации — фаза17

Пользователь подключил Телфин; импорт all_available падал invalid_provider_timestamp до первого checkpoint. На настоящем API timestamps корректны, причина — пустая необязательная confighistory_start. Actualsync_range теперь игнорирует missing/blank/whitespace inputs, сохраняет строгие непустые даты/checkpoints/UTC/guards и прежние defaults30/364days. Неизвестное coverage не становится полным. Успешный retry восстанавливает error→configured для дальнейшего polling, disconnected не активируется.

GSD PLAN-CHECK/revision0 и независимый implementationreview PASS; 5 новых tests, 2 исходные регрессии RED подтверждены. Workerlib/providers/modelHTTP:42actualchecksPASS+1existingPG-case skipped безTEST_DATABASE_URL, новыхSQL/schemaизменений нет. Code2087a86/GitHub и native5images15-2087a86 опубликованы,8productionhealthy. Только исходный failedsync одной компании атомарно повторён с org/job/subject/kind/state/errorguards, receipt1. Импортированы43370уникальныхзвонков, window33/page7; затем TelphinHTTP429 назначил часовойbackoff, jobretry/attempt1. Независимыйverifier подтвердил implementation/release/liveprogress/backoffPASS; completed/configured/scheduledpollingpendingиз-завнешнейквоты, очередь не обходить. См.17-VERIFICATION. DB/secrets/Codexauth сохранены, другие проекты не менялись. Postdeployowner/tenant/media/Codexauth/mandatoryregistrationChrome+mobile/realanalyzertunnelPASS. HumanUATpending; полная историческая база/APIcoverage и ASRquality не обещаются. Phase12paused/stash сохранён.

## История реализации — фаза16

Явно потребована публичная регистрация и подтверждение почты как Bondscan. Immediate-signupdraft отменён пользователем и не публиковался; revisedPLAN-CHECK PASS1/3. Реализованы pendingtable/v2migration безuser/org/session доOTP,6digitcode15мин/resend60сек, nonceORoriginalpasswordproof длядругогобраузера, persistent5attempts/одноразовыйcommit/newemptytenant; существующиепользователи/пароли/данные не меняются. Собственный privateCIDR SMTPbridge кhostPostfixhealthy, nohostMTAchanges/чужихсекретов, SPF/rDNS/protocolpath проверены; Gmailinboxнепроверялся.

ImplementationreviewPASS после2fixes: PWbruteforcecounterприknownOTP, cleanupвнеperemailtransaction+SKIPLOCKED/expiredrecheck. РеальнаяизолированнаяPG+SMTP8/8PASS (6registration+2platform), frontend90/90/typecheck/build, actualMailpitChrome2/2(manualOTP/newbrowserlink+PW/mobile390). RED3frontend иisolatedPGPWcounter+oldproductionregister404 подтверждены. LocalDockerVM былполон; ownisolatedremotePG/Mailpit/SSHfixtures использованыбезprodDB; удалёнтолькоunusedownphase14probeimage, никакихsharedprune/restart. Sourcecode98b53e7/GitHub и все5images15-98b53e7 опубликованы, production8healthy/schema2, независимыйverifierPASS. PublicChromeформы/неверныйOTP400/me401/querystrip/mobile390, старыеownerлогин/аудио/Codex/tunnelPASS; users6/org2/members6/pending0наактивации, ниодногоGmailаккаунтазаuserне создано. Backup18/18включаяновуюPGочисткуpendingOTPприisolatedrestore+REDPASS; manualproductionrestore обязанотозватьproofsдоAPI. Всеownfixturesубраны. ФинальныйhumanUAT/личныйGmailpending, см.16-UAT.md; подтверждениепочтыобязательно, nofakeSMTPsuccess.

## История реализации — фаза15

Production https://phone.bondscan.ru опубликован на разрешённом dks@81.16.141.170; GitHub origin KirDontsov/phone-platform/master подключён, code5789727 отправлен без изменения истории. Собственный Compose рядом с Lentaria/Bondscan,7сервисовhealthy,5nativeamd64images15-5789727; sharedresources не очищались. Независимый PLAN-CHECK PASS0/3, execute completed; независимая implementation VERIFICATION PASS, automatedUAT PASS; humanUAT pending.

Planned transfer только Phone:2org/6users/6memberships/759calls/614readyrecordings/10connections/27471docs/1247jobs baseline. SHA/digests совпали,614readyfiles проверены,5encryptedconfigs расшифрованы прежним encryptionkey, свои Codexauthvolume/confirmedgeneration сохранены. Три demo passwords заменены на случайные; новые доступы только0600 secrets/production-access.json, snapshots private data/backups/production-20261112. Websessions/grants/tempcredentials отозваны. Cutover finalized, старые localAPI/worker/assistant stopped, исходные данные и volumes сохранены.

Собственные nginx/TLS/ACMEwebroot/renewalhook; LEcert до2027-02-10, dryrunrenewPASS. Настоящий remoteDocker→privateSSHrelay→этоткомпьютер→OpenCode1.18.10/catalog2ASRmodels PASS, anon/wrongpassword401, start-analyzer/doctorhealthy. Компьютер/Docker должны работать. Runtimeсекреты/учётные записи Bondscan не использовались; его hostSSHtarget явно передан пользователем.

20migrationtests с отдельным настоящимPG PASS,8REDmutations подтверждены; Linuxstaging/modes/immutableSHA/flock и REDtar-p PASS. Non-rootDocker assertions relay/Codex0.167.2 PASS. PublicPython/Chrome1440/390 PASS:3старых пароля401,3новых входят Secure/HttpOnly/Lax,2компании/header409/foreigncall404,mediaRange206,Codexauthenticatedtrue,JS0/overflow0/configwrites0/modelrequests0. Исправлены реальные deploymentbugs: quotedtmpfs, PUBLICkey644 дляUID10001, сохранение gitpermissions приarchive/defensivecodechmod; releasebuild state/cache на собственном HDD.

HumanUAT/реальная ASRmodelquality и внешниеCRM/Sheets не закрываются transport/browser smoke. OpenRouterключ компании ещё нужен; SMTP отсутствует (invite/reset не доставляются). Проверка владельцем: phases/15-production-deployment/15-UAT.md; эксплуатация docs/release-runbook.md. Phase12/stash ddddf25f4ef384b0a02359042efa4bee159a1f09 сохранены.

## История реализации — фаза14

Уточнена topology пользователя: удалённый сайт/worker вызываетlocalOpenCodeнаэтомкомпьютеречерезDockerSSHtunnel, one-commandstart-analyzerкакпоидееBondscan. ДоступныйстарыйBondscan startscript исследованбезenv/auth/ключей; скриншотсоответствуетболее новойотсутствующейDockerконфигурации. Phonetransportреализованнезависимо, нечерезегоБД/MQ/secrets.

Выполнены4задачи14-01: hardenedDockerconnector/public4096/nativeOpenCode1.18.10localhost4097; ASRadapterнаfixedOpenRouterSTT; exacttwoIDsmoondream/parakeet-ultra+nvidia/nemotron-3.5-transcribe вconfig/UI; tenantproviderkey отдельноотruntimepairedpassword; private remoterelayDockeraliasmodel-runner иlocalstartup/init/status/doctor/down/pin/reconnect. Nativeanalysis/MCP0/tool-deny/authsessionsсохранены. CallerнеперенаправляетtransportpasswordнаarbitraryURL; provider/baseURLchangeтребуетновыйAPIkey. Longaudiochunks300s/24MiB, numericlabelsunknownroles, RAIIcleanupнаотмене.

Наэтомкомпьютере ./start-analyzer.sh local реальносоздалownPhonekeys/pairing/template0600изапустилphone-analyzer-model-runnerhealthy. Tunnelsне запущеныкреальномусерверу: remoteSSHhost/userнепредоставлены, asyncвопросотправлен. Документированодноразовыйrestrictedpairing, незаимствуютсяBondscanaccounts. MainAPI/worker/client/modelrunnerтакжепересобраны/deployedhealthy, другиепроектыиавторизациясохранены.

Research→PLAN-CHECKPASS0/3→execute→independentreview/fixes→verifier→automatedUAT. GSD-toolsplanstructurevalid4tasks. Node17наhost/Docker,nativepin/auth/config/session/MCP0PASS; genuine2hopSSHremote-workercontainer→relay→localgateway/nativehealth+2modelcatalogPASSсnegative/reconnect/rotation/pairmismatch. Worker45(APIHTTP/protocol/providers/PGpipeline/lib),API/PG3,Python10,frontend75/typecheck/buildPASS. Реальныйbrowserread-onlysettings1440/390PASS,JS0/configwrites0/modelrequests0. ОсновныеREDпроверены,остальныеограничениявsummaries/VERIFICATION. TestDB/isolatedSSHfixturesудалены.

Следующийlivegate: SSHhost/user,ownkeyauthorizationиpairingсреальнымсервером,затемключOpenRouterкомпаниииконтрольнаяASRзапись. Реальныхmodel/Telphin/CRMcallsвфазе14небыло. Инструкцияdocs/local-analyzer.md; финальныйhumanUATphases/14-local-analyzer-opencode/14-UAT.md. Acceptedphasesостаются0; phase12/stashddddf25f4ef384b0a02359042efa4bee159a1f09не трогать.

## История реализации — фаза13

Четыре сценария заказчика закреплены в PROJECT/REQUIREMENTS и docs/customer-workflows.md. Реализованы относительный callback X часов, явный checklist, общие Dashboard/Calls filters, теги/статусы/analysisstate, правая панель разметки, полная ширина/mobile и прозрачная ASR→analysis очередь. Preflight не ставит реальные записи на demoASR, active job dedup атомарен. Текущий анализ/оценка/экспорт привязаны к последней расшифровке; старые результаты и pinned delivery сохранены.

Причина прежнего «поставлено в очередь, ничего не произошло» установлена:542 transcribe failed с demo_provider_not_allowed. Рабочая ASR/analysis конфигурация пользователем ещё не настроена. Никакие доступы/критерии автоматически не изменялись, реальные provider requests в фазе13 не выполнялись.

Research → PLAN-CHECK PASS0/3 → execute → independent review/fixes → verifier → automated UAT. GSD-tools plan-structure valid3tasks. Отчёты phases/13-call-workspace-acceptance; финальная пользовательская проверка 13-UAT.md. Код/локальная проверка не равны внешней приёмке. Phase12 и stash ddddf25f4ef384b0a02359042efa4bee159a1f09 остаются приостановлены.

Стенд localhost:18081: API/worker/client пересобраны и deployed healthy; остальные сервисы healthy. PostgreSQL/Actix2, worker/PG7, providers16, APIrubric1, frontend72/typecheck/build, Docker browserfixtures3/3 desktop1920/mobile390. Основные новые RED и4backendмутации подтверждены. Real read-only browser:100rows computed processing, demo blocker visible/transcribe disabled, JS0, no customer writes, own session loggedout. Детали и финальные domain checks в VERIFICATION/UAT.

## История реализации — фаза11

Пользователь запросил встроенный чат как в Bondscan: Codex в Docker через MCP сам подключает Телфин и настраивает статусы, теги, фильтры, шкалу и правила перезвона. Доступы передаются защищённой карточкой внутри чата; модель получает encrypted credential_ref. Новый assistant-runner отделён от анализатора записей, у которого MCP остаётся выключенным.

Контракт: docs/contracts/assistant-v1.md. Research и три плана: phases/11-configuration-assistant. Root — автор плана/API; fix_cli_review — независимый plan-checker; domain_persistence — исследование и реализация runner; frontend_platform — исследование и реализация UI. Модели наследуются. Независимый PLAN-CHECK PASS после доработки1/3; реализация прошла execute gate. API/runner/UI реализованы и проверены локально; финальный пользовательский тест и живые аккаунты остаются отдельной приёмкой.

Проверены Codex npm 0.167.2 и MCP SDK 1.32.2. Host/Bondscan auth не читалась, запросов к моделям не было. Собственная авторизация Codex будет в UI: device code/API key. История ограничена owner/user/org; capability на каждый tool привязан к исходной сессии, run и TTL. Мутации, receipts и audit атомарны, есть CAS/cancel/revoke/deadline. Разрешены восемь типизированных инструментов настройки; нет IAM, retention, export, произвольных HTTP/SQL, shell, репозитория или Docker socket.

Фаза11: Codex0.167.2/MCP SDK1.32.2, отдельный hardened Docker runner,8typed tools, owner chat drawer/fullpage/mobile, защищённые карточки Телфина, собственный model login, durable history/run/events/cancel. Общие settings services/CAS/audit; credentials physical GC/restore revocation. Нет product fake response fallback.

Проверки phase11: API6/6 на настоящих Actix/PostgreSQL/loopbackHTTP (scope,originalsession,generation,receipts,CAS,races,partial/EOF,GC); затронутые HTTPplatform2/2; Node/Docker15/15 включая actual pinned Codex+SDK positive/forcedforbiddennegative с локальным Responses fixture. Frontend43/43 unit/build; assistant E2E mainDocker4/4 +isolatedwirefixture4/4, это5distinctcases на2targets, baselineaffected6distinctPASS. Screenshots1440/390, overflow0, JSerrors0. Python81offlinePASS +actualPGbackup2/2; отдельный backup17/17включаяэти2. Полный create→isolatedrestore повторPASS,22ready media, auth/capabilities revoked, ciphertextpurged, noresurrection. Машинные отчёты приватны в data/eval/phase11 и data/eval/assistant-*.json.

Независимые review findings исправлены и проверены: authgeneration TOCTOU, порядок сообщений, metadata, failed tool budget/replay, DB lock inversion, nested schema parity, false uncertain status наcompleted, неполный settings snapshot. Research/plan-check/summary/verification/UAT — phases/11-configuration-assistant. Первая planrevision1/3 PASS; implementationverification отдельно.

Стенд http://localhost:18081 запущен. Проверка владельцем: открыть «Помощник» →войти в Codex кодом/APIkey →«Добавить доступы» для Телфина →обычная задача текстом. Настоящий model/device/Telphinlive и humanUAT pending; платных внешних model requests0. База v1 и её внешние проверки сохраняются.

## База v1 до фазы 11

Веб-платформа «Сигнал» реализована и запущена в независимом Docker project phone-platform: http://localhost:18081. Учебный владелец owner@demo.local / PhoneDemo!2026, две изолированные компании. API/worker/client/PostgreSQL/RabbitMQ/Mailpit и необязательный OpenCode runner healthy. Повторный запуск: `scripts/dev.sh up`.

Пользователь поручил автономно выполнить все фазы и оставить ему финальный тест. Разработка не останавливалась на live gate M0. Предыдущее указание сначала работать без живого Телфина соблюдено: реальные Телфин/ASR/LLM/CRM/Google не вызывались. Credentials и данные Bondscan не использовались.

Все 26 планов имеют реализацию или явный mapping к фактическим файлам и описанные архитектурные отклонения. Для 10 фаз созданы SUMMARY/VERIFICATION/UAT; implementation baseline готов, внешний и пользовательский acceptance не объявляются complete. `completed_phases=0` отражает отсутствие финального подтверждения, а не отсутствие кода. `completed_plans=1` сохраняет строгий статус полностью закрытого CLI-плана; частично внешние acceptance criteria остальных планов не подменены локальными тестами.

## Проверки

- Rust: 47 различных PASS, включая отдельные PostgreSQL/Actix и loopback HTTP checks; workspace compile без warnings. После последнего purge fix повторён затронутый pipeline, не весь workspace без необходимости.
- Frontend: 20 unit PASS, vue-tsc/Vite build PASS; полный Docker/nginx E2E 13/13 без skips, 44.1s. После access audit/DNS исправлений affected2/2 повтор PASS14.8s, actual audit events и смена Docker API IP без рестарта client проверены. Desktop1440/mobile390, overflow0, JS errors0, audio32s/ready4/errornull.
- Telphin CLI: 56 offline PASS; live requests0.
- Python: 80/80 PASS = CLI56 + backup14 + evaluation10.
- Backup: safety tests и настоящие create/isolated restore PASS; 22 ready media, 5 документов удалённого после backup звонка не восстановлены, ledger1, shared audio сохранено. Исходная БД не подменялась.
- Clean Compose: новые случайные БД/тома, миграции, auth2companies, private audio Range206, очередь→текст→анализ и foreign call404 PASS; тестовый project удалён.
- OpenCode1.18.10: реальный Docker deny-all/auth/session smoke PASS, model prompts0. Quality harness ASR/analysis реализован, без reference выдаёт acceptance=null.
- Load: 5000 synthetic calls/concurrency4, p95 списка616ms, dashboard752ms; это небольшой локальный профиль, не production SLA.

Актуальные детали и воспроизводимые команды: docs/pilot-report.md, .planning/research/EXECUTION-DELTA.md, .planning/reviews/IMPLEMENTATION-REVIEW.md, .planning/reviews/FRONTEND-VERIFICATION.md, .planning/reviews/FINAL-VERIFICATION.md. Машинные отчёты в приватном Git-ignored data/eval.

## GSD и решения

Research → 26 plans → независимый PLAN-CHECK → 2 baseline revisions → execute → implementation review/fix/regression → verifier → automated UAT. Финальный human UAT оставлен пользователю по его указанию. PLAN-CHECK не считается проверкой готового продукта. Commits: e789168 платформа; 7af83b7 backup/eval/smoke; 3563c4f access audit/DNS; 0656c50 CLI fixes. Последний runtime manifest: data/eval/release-manifest.json.

Стек Bondscan сохранён, сервис и данные независимы. Composite tenant FK + membership session guard, durable DB jobs/fencing + Rabbit outbox, private media volume, encrypted provider configs, immutable transcript/rubric/analysis, manual locks, strict AI evidence. Конкретные решения: docs/architecture-decisions.md, docs/contracts/platform-v1.md.

## Осталось для внешней приёмки

Финальный пользовательский тест по docs/uat.md и настройка разрешённых аккаунтов. Нужно сверить полноту/границу истории Телфина, качество русского ASR/диаризации/оценок на reference corpus, фактические OAuth/scopes/доставку CRM/Sheets и тарифы моделей. Публичный deployment target не задан и не публиковался. Вопросы пользователю не задаются: открытые параметры описаны как настраиваемые предположения.

Отклонения: polling5min с overlap вместо webhook lifecycle; фильтры/агрегаты пока Rust in-memory вместо SQL read models; единый retention компании. Никакое неподтверждённое внешнее качество не называется PASS. Следующее действие — пользовательский финальный тест готового стенда, затем исправление конкретных замечаний.

## Повторное code review — 2026-11-12

Исправлены: передача названий тегов/статусов модели; сохранение unknown-исхода доставки через ошибку reconciliation/credentials и retry; явное отключение ASR/analysis без demo fallback. Подробнее reviews/IMPLEMENTATION-REVIEW.md, раздел2026-11-12. Свежие worker33 (включая настоящую отдельную PostgreSQL), core15, Python80, frontend20 PASS; новые4 regressions, совокупный набор Rust51 (API2/persistence1 — прежние результаты, не новый повтор). API/worker Docker пересобраны; полный browser13 — прежний baseline, текущий targeted smoke проверяет новые версии анализа и сохранение ручных полей. Короткая пользовательская проверка: docs/uat.md → «Быстрая проверка». Внешние аккаунты не использовались.

## Исправление пользовательского UAT: startup Codex — 2026-11-12

Пользователь вошёл в Codex, но любой prompt падал. Причина: authenticated startup присылает informational remoteControl/status/changed, отсутствовавший в строгой event allowlist. Исправлены известное notification, first-error preservation и safe stage/code diagnostics, API/UI error classes, duplicate connecting status и misleading uncertain для обычных status. Запрет unknown tools/events/RPC сохранён.

Свежие Node19/19 и affected API2/2 PASS, frontend43/43 PASS; native positive/forbiddennegative теперь проходит полный createRunnerHTTP→TurnService, не обходит orchestrator. Реальный запрос через уже подключённый ChatGPT account завершён: gpt-6.1-sol ответил «готово». Проверочный чат удалён, настройки и авторизация сохранены. Прежние заявления model requests0 относятся к baseline до этой live проверки; Телфин/анализ речи/CRM всё ещё не проверялись. Подробности phases/11-configuration-assistant/11-STARTUP-FIX.md.

Дополнительный live MCP дефект также исправлен: модель с tool-search catalog получала настройки как deferred без доступного механизма поиска. Ровно8Phone Platform tools теперь direct-only (`omit_tools_from=[deferred,code_mode]`); встроенные OS/code-mode инструменты по-прежнему выключены. Настоящий get_workspace_configuration через gpt-6.1-sol завершилсяcompleted, revision0 сохранена, контрольный чат удалён. Docker/API/client пересобраны/healthy, пользовательский вход сохранён.

Startup followup закрыт локально: Node20/20 вhost/Docker, knownproductionmodelcatalog +fullorchestrator, forcedOS/JS/edit denied, affectedAPI2/2, frontend43/43. Пользователь затем уточнил новый UXgap: есть логины кабинета и АТС, а неAPIAppIDSecret. Начинается фаза12 — получениеAPIприложения через защищённый PBX onboarding. Текущий API-only способ остаётся advanced, новый пользовательский ввод не должен требовать неизвестных емуAPIполей.

Пользователь остановил переделку («Нет, погоди») и уточнил: не знает, где получить APIдоступ. Фаза12 приостановлена; plancheck PASS не означает реализацию. Написанные draftform/adapter файлы сохраняются, не опубликованы и не считаютсяпроверенными. Dockerостаётсяна рабочем startupfix d013faf (Codexreply+MCPreadPASS,20Node/nativechecks). Следующееобщение — инструкцияпоофициальному app/register, беззапросасекретов.

## Client ID — узкий пользовательский фикс

Commit ef3f00e: App ID/App Secret достаточны в APIформе чата и настройках; отсутствующий/пустой Client ID определяется только по authenticated /user/. DeployedAPI/worker/clienthealthy;32worker+4API+45frontend+actualbrowser1PASS. Основные missing-IDregressions REDподтверждены, securitynegativeRED отдельнонепроверяли. Настоящие ключи Телфина пользователем ещёнепроверены. Детали phases/11-configuration-assistant/11-CLIENT-ID-FIX.md.

Phase12implementationdraft сохранён в отдельном Gitstash `ddddf25f4ef384b0a02359042efa4bee159a1f09` («Paused phase12 implementation; API Client ID fix remains in main»), вmainоставлен только работающий API-key путь. Plans/researchсохраняются; phase12неисполнятьбезвозобновленияпользователем. Stashнеудалятьдоуспешногоresume/review.

## Записи: лимит API и ожидание плеера

После liveимпорта ready391real(+17demo),pending126; provider429→retryна13:00UTC. ReadyfileпроверенHTTP206иmutedbrowserplayback. ИсправленAudioPlayer: pending/ratelimitedнеоткрываетmedia иобъясняетожидание, readyпоpollingвключаетсяавтоматически, готовыйфайлвыбираетсябезсбросавоспроизведения. Frontend51/51,5newbehaviourregressionsRED,actualbrowserready+pendingPASS. Clientdeployed. Подробностиreviews/RECORDING-AVAILABILITY.md. Фаза12остаётсянапaузе; лимитproviderобходомне снимается.

## Доступность записи в списке

CALL-01/CALL-03: вычисляемый recording_state, колонка «Запись», мобильный badge и быстрый фильтр по6 состояниям. «Можно слушать» означает завершённую загрузку; has_recording/stage не подменяют доступность. Один tenant-scoped bulk read без N+1. API/PG1 интеграционный PASS с17 случаями и RED без фикса, frontend56/56 и5 новых RED, domain14/14, cargo check/build PASS. API/client deployed healthy; реальный browser ready-filter507total/25rows, mobile390 без overflow, JS0. Детали reviews/RECORDING-LIST-AVAILABILITY.md. Финальный human UAT остаётся пользователю; фаза12 не возобновлялась.
