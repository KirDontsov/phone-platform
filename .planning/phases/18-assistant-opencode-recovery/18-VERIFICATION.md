# Phase18 — независимая проверка реализации и выпуска

Дата: 2026-11-12. Проверяющий: timestamp_plan_checker. Автор компонентов/выпуска — root; автор auth staging — assistant_auth_diagnosis; UI — opencode_settings_ui.

**Результат: implementation/release PASS. Пользовательская приёмка pending; рабочий APInex inference НЕ подтверждён — внешняя квота402.** Личный OAuth/диалог Codex и завершение полного импорта Телфина остаются отдельными gates; этот отчёт их не закрывает.

## Требования и действительное evidence

| Проверка | Результат |
| --- | --- |
| Codex remote403 → локальный authenticated CONNECT | PASS: настоящий pinnedCLI initialize/device challenge проходит через service childEnvironment; код не публиковался, изолированный home/process удалён |
| Failed reauth сохраняет прежнюю authority до commit | PASS: runner/реальный API-PG tests, prepare/commit receipt, logout/latecallback; crash promotion marker проверен двумя restart/fault tests |
| Existing OpenCode credentials без повторного ключа | PASS для импорта/tenant-bound catalog/проверки доступа; **APInex inference BLOCKED quota402**, другой провайдер не выдуман и автоматически не подставлен |
| Tenant/секреты/native routes | PASS: wrong/missingorg denial, session ownership/modelallowlist, public DTO/redaction, сервер задаёт org; live чужой catalog unavailable/config403, ключи скрыты |
| Явный ASR/local vs OpenRouter | PASS: собственный APInexVoiceSTT ID, исходные два OpenRouter IDs сохранены, нет auto-selection/save/processing, local APIkey field отсутствует |
| Сохранность Телфин retry при rollout worker | PASS: activeleases0 перед replace, before/after совпадают; независимый readonly check подтверждает прежний checkpoint/backoff |
| Runtime/HTTPS/browser/auth/media/tunnel | PASS:8 healthy и correct tag, публичные smokes/desktop+mobile/settings/OTP и настоящий tunnel roundtrip |

## Независимое review и исправления

PLAN-CHECK revision1/3 PASS; root и auth review выполнены отдельно до публикации. Закрыты замечания: shared pairing не переносится на внешний analysis destination без свежего ключа, session capacity резервируется до upstream create, local URL нормализуется до pairing injection. Auth promotion теперь предварительно сохраняет authenticated=false/commit_uncertain marker: после crash/post-rename failure новая credential не исполняется под старой generation. Backup/receipt/logout handling сохраняет fail-closed при неизвестном commit. Подробности:18-ROOT-REVIEW.md и18-AUTH-REVIEW.md.

## Сборка и выпуск — PASS

Активирован только runtime15-d59a82e, sourcecode d59a82ef1472fa36f4aacddea70e6c9fe7e87bff. Предыдущие phase18 images0a2ab0e собраны, но не активировались. Final build evidence содержит пять linux/amd64 images; проверяющий самостоятельно прочитал docker ps на разрешённом сервере: все8 Phone containers healthy, все5 appimages ожидаемого15-d59a82e тега. GitHub master в момент проверки равен d59a82ef; финальный documentation commit root проверяет отдельно после этого отчёта. Phase12 stash ddddf25f4ef384b0a02359042efa4bee159a1f09 сохранён.

В activation evidence worker_active_leases=0. phase18-sync-before/after идентичны: job retry/attempt1, next_attempt_at2026-11-12T20:00:00.167468Z, window33/page7, imported43371/unique calls43370, provider_rate_limited. Проверяющий независимо выполнил только readonly phase17 sync check после rollout и получил те же значения. Ни force retry, ни изменение next_attempt_at/lease/checkpoint, ни сброс данных не выполнялись. Полнота истории и восстановление configured/следующий polling ещё **не проверены** из-за provider backoff.

DB schema/ключи/auth volumes/SSH pins/pairing сохранены, новых restore/ротаций в diff нет. Полная checksum-сверка всех изменяемых пользователем данных и здоровье соседних проектов заново в этой фазе не проверялись; изменения выпуска ограничены собственным Phone Compose. Проверяющий не читал runtime credentials и не выполнял production mutations/logins/provider inference/ASR.

## Реальный транспорт и границы провайдера

Просмотрены actual probe source и JSON: `mode=service` вызывает штатный childEnvironment из deployed assistant image без ручной подмены proxyenv; pinnedCLI инициализируется и получает настоящий device challenge вместо ранее подтверждённого remote403. Исходный initial/manual probe сохранён отдельно. Проба не создаёт Phone login, не использует чужой auth и не выполняет turn; одноразовый challenge не напечатан/не сохранён. Повторный challenge проверяющий не запрашивал. Исследование pinned WebSocket source подтверждает соответствующий proxy-aware code path; upstream WebSocket tests в этой среде не запускались, настоящий authenticated Responses остаётся UAT.

Live local-provider evidence: own catalog available, freellmapi/apinex,29 model entries; foreign catalog unavailable, native config403, ключи скрыты. Этот metadata probe выполняет0audio/0inference requests. Отдельные контролируемые free text probes автора без customerdata/audio не дали model answer; исследован native APIError402. APInex metadata аутентифицируется, но allowance недоступен; текущая local/test честно возвращает provider_payment_required. **Нельзя объявлять работающими ни APInex inference, ни ASRquality, ни иной неуказанный пользователем провайдер.** Новый ключ/платный switch/пользовательские настройки автоматически не записывались.

## Автоматические тесты

Финальные evidence автора просмотрены: frontend95/95 + typecheck/Vite build; gateway26/26; assistant26/26; worker19lib+11HTTP+16providers (46 отображаемых ok, **45 фактически выполненных и1 существующий PG-case пропущен** без TEST_DATABASE_URL); APIunits3/3, actual isolated PG/HTTP ASR1/1 и auth2/2; importer4/4. Наличие PG/HTTP отдельных настоящих fixtures не превращает пропущенный lib-case в выполненный.

Проверяющий самостоятельно прогнал gateway/localproviders/proxy25/25 до quota supplement, затем localproviders5/5 с quota supplement; assistant runtime/proxyenv22/22 с crash tests; pairing guard unit1/1 и native402/mandatoryDELETE HTTP regression1/1. Остальные final suite evidence просмотрены, их повторный полный запуск не заявляется.

RED evidence просмотрено: пять UI tests на прежнемUI; настоящие PG mutations analysis pairing guard (200вместо400) и keyless preflight(falseвместоtrue); gateway tenant/redaction/sessioncapacity3 mutations; quota1 mutation; auth failed reauthorization, proxy auth/privateDNS/env. Capacity bug дополнительно независимо воспроизведён до исправления129 nativecreates приcap128. Проверяющий не запускал отдельные RED mutations persistent crash marker и native402 mapper; не заявляет, что каждый новый assert отдельно проверен RED.

## Публичная проверка и оставшийся UAT

Postdeploy smoke: старые demo passwords3/3 отклонены, текущие operator logins3/3 PASS, Secure cookie/tenant header guard/foreigncall denial, ciphertext hidden, mediaRange, старый owner assistant authenticated. **Это другой tenant; личный новый OAuth пользователя данным smoke не подтверждён.** Analyzer doctor: настоящий SSH/HTTP roundtrip/pairing и оба local containers healthy.

Public Chrome evidence: registration+обязательныйOTP/invalid400/me401/querystrip, desktop1440/mobile390, JSerrors0/overflow0, register submits0/emails0. Settings browser: public settings, local analysis/ASR APIkey fields отсутствуют, legacyOpenRouter отдельно, foreign local catalog unavailable, auto-selectedlocalASR=false, configwrites0, JSerrors0/overflow0 на1440/390. Эти browser/smoke artifacts просмотрены проверяющим; новых login/browser actions он не выполнял.

Осталось: пользователь завершает собственный device OAuth и проверяет диалог; подтверждается фактически выбранный существующий provider/его allowance и разрешённая контрольная ASR; после квоты Телфина проверяется исходный completed sync/configured/polling. Эти внешние ограничения явно остаются pending в18-UAT.md/STATE. По плану реализация проверена, полная пользовательская приёмка не объявляется завершённой. Чеклист Григория: не проверял, нет чеклиста.
