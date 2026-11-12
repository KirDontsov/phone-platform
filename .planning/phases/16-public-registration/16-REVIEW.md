# Independent implementation review — Phase16 revision1

**IMPLEMENTATION REVIEW: PASS после исправления двух замечаний.** Проверяющий `/root/registration_plan_checker`, автор backend `/root`. Предмет: обязательное подтверждение email; прежний immediate-signup draft не оценивается как готовая функция. Source reviewed: registration/auth/main, explicitmigrationv2/schema, frontendregister/verify/store/routes, productionSMTPrelay, HTTP/PG/SMTPtests. Production feature ещё не опубликована на момент этого review.

## Найденные замечания

1. **RESOLVED — пароль crossbrowser не расходовал persistentattempts.** Изначально неправильный pendingpassword возвращал Invalid до счётчика; обладатель верного OTP мог распределить password guesses между IP, обходя общие5попыток challenge. Исправление root проверено в source: обе проверки выполняются в blockingworker; если хотя бы nonce/password либо OTP верен, неверный второй factor расходует5persistentattempts. Полностью посторонние запросы с обоими неверными factors не блокируют чужой challenge. Ошибочный Outcome возвращается после commit, HTTP не выдаёт factorflags. Новый actualPG/SMTP `cross_browser_password_attempts_are_persistently_bounded` PASS:5неверных паролей с верным OTP дают attempts=5, затем обе верные величины уже не активируют identity.

2. **RESOLVED — global cleanup под peremail advisory lock.** Изначально reserve(emailA) делал globalDELETE чужих expiredrows до load своей строки; две заявки A/B могли получить циклические rowlocks иошибку. Root убрал cleanup из reserve, оставил backgroundmaintenance и добавил `FOR UPDATE SKIP LOCKED` в bounded100 cleanup, outer expiry-condition повторно проверяется. Удаление конфликтующего lock order подтверждено source review; отдельный cleanup deadlock runtime-test не заявляется. ActualPG concurrentregister/verify и forcedtransactionrollback PASS.

## Положительные проверки исходников

- До SMTPack register возвращает только pending202/registration-cookie, без appsession/Me. User/org создаются только после актуального sentchallenge, expiry/attempts и двухфакторной проверки (emailOTP + nonce либо исходный пароль). Новый owner получает только новую пустую company.
- Pending idUUID служит sendgeneration: поздний SMTPack обновляет только свой id и sending-state; staleack не может перевести новый challenge в sent. SMTPfail возвращает503 и failedstate; retry не создаёт identity заранее.
- Atomicsingleuse transaction включает useruniqueconflict, org, owner, audit, session и challenge DELETE. Existingemail не upsert и не объединяет организации. Schema migrationv2 явно wired в прежний advisorymigrationtransaction.
- Code/password/nonce хранятся hash-only, поля denyunknown, origin/IPguards/10kmapcap доCPUwork. Нет rawOTP/password вHTTPerrors и обычных логах. Host/client accesslogs `$uri` безquery, document referrer-policy no-referrer.
- VerifyEmail UI удаляет email/codequery до autoverify, секреты остаются в componentmemory, в sessionStorage только email. Pending202 не adoptsMe; passwordfallback добавляется только при409; success200 использует прежнюю server-cookie модель. Existinginvite/reset/login retained.
- SMTPrelay pinned, nonroot/read-only/capdrop/32MiB/0.25CPU, hostnetwork bind только ownbridge1587 и range только Phonesubnet; hostPostfix и чужие секреты не меняются. Источник actualartifact подтверждает EHLO250 ownnetwork, foreignbridgeDENY, public1587closed. Это не доказательство доставки вGmail.

## Evidence / оставшиеся gates

Прочитаны frontend90unit/typecheck/build и3RED artefacts; новыеregister/verify15unit PASS. Прочитан actualSMTPrelay JSON: privateACLpositive/negativehealthy, реальные письма не отправлялись. Финальный `data/eval/phase16-registration-tests.log`: actualPG/SMTP registration6/6PASS плюс existingplatform2/2PASS; проверяющий независимо сопоставил исходники assertions с логом, повторно не запускал suite во время browserE2E на той же fixtureDB.

Первоначальное падение forcedrollbackcase503vs500 было ошибкой test expectation: существующий `ApiError::internal()` возвращает503 `service_unavailable`. Исправление expectation не ослабило тест: после forcedDBfailure подтверждены неизменные counts, затем тот же proof повторно успешно создаёт identity и concurrentverify даёт ровно200/400. `.min_idle(Some(1))` в общем pool сохраняет max12/timeout10s и уменьшает coldstartup ожидание с12до1соединения; inspected, новыйAPIfixture ready. Это не изменение auth-контракта.

Browseractualmailflow и productionverifier пока не завершены. Этот reviewPASS относится к исправленной реализации и указанным actual test artefacts, не к production/inboxdelivery или пользовательскому UAT.

NonblockingUX: resend в другом браузере без nonce честно возвращает условное сообщение и не отправляет письмо; полезна подсказка вернуться в браузер регистрации/начать заново. Это не причина ослаблять browserbinding.
