# Phase16 — independent implementation verification

**IMPLEMENTED / AUTOMATED VERIFIED. Human UAT и доставка в личный Gmail — pending.**

Независимый verifier: `/root/registration_plan_checker`. Release code `98b53e7`, production images `15-98b53e7`, сайт `https://phone.bondscan.ru`. Устаревший immediate-signup draft не опубликован: production требует email proof. План revised PASS1/3; два implementation review замечания устранены.

## Независимые проверки реального production

- По SSH получены только безопасные Docker/schema/file metadata: **8сервисов healthy** — API,worker,client,assistant-runner,model-tunnel,PostgreSQL,RabbitMQ,SMTP-relay. Все пять app images — `15-98b53e7`; внешний health нового SMTP-sidecar healthy. Ресурсы других проектов не менялись verifier.
- Schema versions `[1,2]`, `users=6`, `organizations=2`, `memberships=6`, `registration_challenges=0`: никаких выдуманных Gmail-аккаунтов, owner membership или pending mail-заявок verifier не создал. Migrationv2 явно wired в прежний advisorytransaction, старые user rows не переводятся на новую обязательную проверку.
- Public `/register` и `/verify-email` — HTTPS200, curl TLS verification0; соседние Lentaria/Bondscan также HTTPS200/TLS0. Неполный registerbody400, неизвестный verifycode400, затем `/api/auth/me`401 без appcookie. Unknown resend200 условный и без письма. Эти negativeprobes не создают identity/challenge и не отправляют реальное письмо.
- Независимо повторён `scripts/smoke-production.py`: anonymouscalls401,3старых demo passwords401,3новых входят, Secure/HttpOnly/Lax,2компании, headerguard409/foreigncall404, Range206,5maskedconnections, Codex runner ready/authenticated true. Созданные probe sessions разлогинены; конфигурации/звонки не менялись, modelrequests0.
- Из **реального Phone Docker network** повторён SMTP connect/EHLO/QUIT через gateway `172.26.0.1:1587`: EHLO250, **MAIL/RCPT/DATA не отправлялись**. Actual Docker network subnet `172.26.0.0/16` совпал с relay ACL. Runtime UID10001, readonly=true, capdropALL, hostnetwork; listener привязан к собственному bridge, sourceartifact дополнительно подтверждает foreignbridgeDENY/public1587closed. MTA/чужие SMTPcredentials не читались и не менялись.
- `./start-analyzer.sh doctor`: настоящие HTTProundtrip/pairing PASS, localmodel-runner/tunnelshealthy. Внешние appAPI/DB/Rabbit/relay ports остаются loopback/internal; SMTP1587 не опубликован на public interface. Собственные env/pair0600, bindPUBLICkey0644 сохранены.
- Git local/remote codeHEAD `98b53e758bb633d384f16a6c231213dbb9cc3e2c` совпали на момент этой проверки; phase12 stash `ddddf25f4ef384b0a02359042efa4bee159a1f09` сохранён. Финальные docs и backup-helper проходят отдельный последующий commit/push root.

## Полный положительный сценарий и regression evidence

Verifer независимо сопоставил source/assertions и сохранённые actual reports; дорогие общие DB-suites повторно не запускались параллельно browserfixture, чтобы не изменять baseline count assertions.

- `phase16-registration-tests.log`: **8/8 PASS** =2existingplatform +6registration настоящие Actix/PostgreSQL/SMTPsink. До OTP нет user/org/session; readback OTP из реально принятого MIME, login pending401, validOTP+boundnonce/newbrowserpassword создаёт только собственную пустую компанию; duplicate/races/privilege injection/origin/expiry/replay/resendrotation/cooldown/SMTPfailure/rollback и пять persistentpasswordguesses проверены.
- ForcedDBfailure503 соответствует старому `ApiError::internal/service_unavailable`; counts не меняются, затем тот же proof может успешно повториться, concurrentverify даёт ровно200/400. Старое ожидание500 было исправлено как test mistake, assertion atomicity не ослаблен.
- `phase16-registration-real-e2e-final.json`: **2/2 actual Chrome+Mailpit PASS**, skipped0/flaky0/unexpected0. Реальный browser register202→emailOTP→verify200→emptyworkspace→logout/login, duplicate409; настоящий magiclink очищает query до POST и в другом browser требует исходный пароль. Это изолированная реальная SMTP-проверка, не synthetic response вместо отправки.
- `phase16-public-browser.json`: фактический production Chrome1440/390, login→registrationfields, обязательная6digitформа, invalidcode400/noidentity401, query stripped, overflow0/JSerrors0, registration submits0/emails0.
- Frontend **90/90 PASS**, typecheck/Vitebuild PASS; **3RED mutations** подтверждены: pending-as-session, autoPOSTдоquerystrip, забытый crossbrowserpassword. Backend separate-source mutation без passwordcounter провалила actualPGtest (exit101), active/production code не мутировался. Missingroute404 в прежнем production был исходным RED; опубликованный route теперь существует.
- Lifecycle sourcefix `REGISTRATION_REVOKE_SQL` проверен: isolatedrestore удаляет pending signup proofs, guard `to_regclass` сохраняет совместимость со старой schema1, существующие users/orgs не меняются. `phase16-backup-tests.log`: **18/18 actual isolatedPG+backup tests PASS**, mutation без DELETEpending провалила новый тест. Это защита пути isolatedrestore `backup.verify`; самостоятельный операторский productionrestore должен применить ту же revocation до старта API, не считать его автоматически реализованным этим helper.
- Приватный pre-migration DBsnapshot на собственном HDD с SHA256 сохранён до schema2 (`phase16-predeploy-backup.txt`). Dump/SMTPbody/OTP/password/customerphones не попали в Git/обычные логи.

## Границы приёмки

SMTP transport настроен на настоящий hostPostfix через отдельный Phone relay. Пробы EHLO/QUIT и SMTPsink **не доказывают поступление в Gmail, папку «Входящие», SPF/DKIM delivery result или чтение письма пользователем**. Реальные сообщения на личный адрес verifier не отправлял. Пользователь должен зарегистрироваться со своим паролем, получить6digitписьмо/ссылку и подтвердить почту — это финальный humanUAT.

Новое подтверждение требуется только для новых public registrations; старые аккаунты/компании/Телфин/Codex данные сохранены. Новая регистрация создаёт пустую изолированную компанию, не присваивает прежнюю по адресу/названию/домену. Model inference/реальная ASR quality и CRM/Sheets не пересматриваются в этом auth release. Phase12 остаётсяpaused. Чеклист Григория отсутствует — не проверял; Figma/Jira источники к этой задаче не предоставлены.
