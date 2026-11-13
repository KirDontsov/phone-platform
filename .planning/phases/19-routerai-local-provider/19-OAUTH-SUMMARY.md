# RouterAI OAuth helper — execute evidence

Реализован `scripts/connect-routerai.py`: одноразовый вход существующим аккаунтом через RouterAI PKCE, без поля/копирования ключа в Phone. На настоящую авторизацию helper этим агентом не запускался; покупок, inference, отправки записей не было.

CLI: `python3 scripts/connect-routerai.py --organization <UUID> [--source ~/.config/opencode/opencode.json] [--project phone-analyzer] [--no-browser] [--timeout 300]`. В stdout появляются только public authorization_url и фиксированные состояния/ошибки, без verifier/code/key. `--no-browser` позволяет root подготовить процесс и передать пользователю ссылку; согласие не подменяется ожиданием/таймаутом.

Callback слушает только127.0.0.1/ephemeralport, проверяет точный Host и path, state, issuer, отсутствие дублирующихся/неподдержанных queryпараметров, TTL. Корректный callback отвечает303 на чистый `/connected` до exchange; логи HTTP выключены, ответы no-store/no-referrer. Неверный callback не расходует попытку. Отдельные consumed/exchange_started guards блокируют повторы даже при timeout. Exchange фиксированный HTTPS endpoint, NoRedirect/TLS verification/20sec/response bound, ровноодин HTTP attempt: официальный RouterAI предупреждает, что повтор использованного code+verifier отзывает выданный ключ.

Commit: тотже `secrets/local-opencode.lock`, чтение CURRENTprivatefile уже подflock, повторнаяпроверкаorg, mergeтолькоrouterai с текущимиproviders. Установка в собственныйvolume через существующийinstaller; hostfile atomic/fsync/0600. Приошибкеinstall/write прежнийhostfileсохраняется, выполняется best-effort rollback именнотекущегоvolume, ошибканеопределённогоrollback явноотмечается. Нетобещанияраспределённойатомарностиfile+Docker при аварийном SIGKILL. Послеуспеха рестартуетсятолькособственныйmodel-runner, ограниченныеrestart/healthtimeouts. Чужаяcompany/symlink неперезаписывается.

app_name — собственное `Phone Platform Local / <fullorgUUID>`, поэтому не перевыпускается ключ OpenCode/Bondscan. Повторный consent тому же собственному Phoneapp/аккаунту может перевыпустить именноегоRouterAIключ на стороне провайдера ещёдо install; сохранениестарогоhostfile не гарантирует валидностьстароговнешнегоключа втакомслучае. Этоофициальноеповедение, неоператорскаяпокупка/неподтверждённаяподменааккаунта.

10tests с настоящими loopback HTTPcallback/exchange fixtures: success/PKCE/303/nosecretoutput, wrongHost/path/state/Unicodeissuer/duplicates, expiry/denial, actualtimeout+replay, noRedirect, lockedcurrentmerge+parallelprovider, tenant/symlinkdenial, installerrollback, failedatomicwrite, actualcurrentimporterRouterAIonlyintegration. GREEN: `data/eval/phase19-oauth-helper-green.log`.

6изолированныхsourceREDmutations подтверждены: убратьstatecheck, callbackoneshot, tenantbinding, менятьruntimeдоinstall, следоватьexchangeRedirect, убратьsingleexchangeguardпослеtimeout. НужныереальныеHTTPassertions падают; рабочиесourcesиruntime не мутировались. Evidence `data/eval/phase19-oauth-helper-red.json` и отдельныеlogs. ИсходнаяResourceWarningзакрытаявнымHTTPError.close; warnings/сообщенияпровайдеранесодержатсявпубличномвыводе.

Независимый review и живой пользовательский OAuth — отдельные gate root; auth/inference успех не объявляется по fixtures.
