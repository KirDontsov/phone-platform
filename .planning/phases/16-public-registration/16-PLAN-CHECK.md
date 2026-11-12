# Independent plan check — Phase16

**Текущий gate: PASS, revision1/3 — обязательное подтверждение email.** Дата2026-11-12. Проверяющий `/root/registration_plan_checker`, автор плана `/root`.

## Revision1 — новый обязательный scope пользователя

Первоначальный gate ниже относится к немедленной регистрации без SMTP и **superseded** новым требованием. Его нельзя использовать как разрешение деплоить старый immediate-login draft. Новый plan/research и schema/session/mailer/proxy/migration contracts независимо перепроверены; структура valid=true,3tasks,errors0,warnings0.

Новый gate PASS:

- Pendingchallenge хранится отдельно от users/orgs. До validproof нет user/org/membership/appsession. После доказательства требуется единая транзакция user→org→owner→audit→session→challenge consumption, поэтому сам факт отправки письма или открытия unsolicitedlink не даёт доступа.
- Шестизначный OTP15min/5persistentbadattempts/resend60s, hash-only хранение, noncehash и SMTPgeneration/sendstate предотвращают простой replay/staleack. Случайный HttpOnly browsernonce либо доказательство pendingpassword в другом браузере плюс OTP защищает от pre-hijacking известным атакующему pendingpassword после случайного emailclick жертвы.
- Неверные попытки должны коммититься отдельным Outcome до HTTPerror; concurrency locks/fencing и forcedDBrollback/race tests предусмотрены. Resend требует ownnonce, неизвестный email/чужой nonce не отправляет письмо и не меняет challenge. SMTP503 не выдаётся заsuccess; failedgeneration не подтверждается.
- Реальный SMTPtransport изолирован ownPhone hostnetworkrelay, bind только privatebridgeIP1587, ACL только ownPhoneCIDR, upstream127.0.0.1:25. Отсутствует изменение hostPostfix/mynetworks или чтение чужих ключей. Mailboxarrival/DKIM не объявляются доказанными. Проверены также публичные DNS TXT phone.bondscan.ru→CNAMEbondscan.ru→SPF ip4:81.16.141.170; это не inbox delivery proof.
- Migration0002 должна быть явно добавлена в hardcoded persistence::migrate/version2 внутри прежней advisorytransaction. Недостаточно положить SQL в папку; parent подтвердил wiring, файл crates/persistence/src/lib.rs внесён в plan.
- UI202pending не adoptsMe и не переходит в dashboard. Querymagiclink strip before submit, code/password только в памяти; crossbrowser409 показываетpasswordfield. После200Me обычнаяsession/adopt. Существующие login/invite/reset и все старые identities не переводятся на новую неподтверждённую схему.

Указания execute/verifier для revision1:

1. Verify допустим только для `sent` актуальной generation с `expires_at>now()`; SMTPack послезамены challenge не возвращает старое sent-state. Out-of-order физическая доставка писем не равна успешному подтверждению старого кода.
2. Ограничения по IP/суммарной map действуют и на verify/resend до дорогого hashwork; persistentattempts нельзя откатить через возврат Err из transaction. Отдельно проверить отсутствие user/org после failed/locked/expired/SMTPfailed states.
3. Проверить реальный dynamic bridgegateway/CIDR из ownDocker network, foreignsourceDENY/publicportclosed, не полагаться только на пример172.26.0.0/16. Отправка в изолированный SMTPsink допустима; реальную Gmail-доставку пользователя подтверждает его UAT.
4. Secret frontendstate не записывать в local/sessionStorage/URL. Magiclink содержитcode по образцу пользователя, поэтому HTTP accesslogs должны использовать `$uri` безquery (текущиеhost/clientconf это уже делают).

Это revised plan-quality gate. Он не объявляет implementation/deployment/actual inbox delivery завершёнными.

---

## История superseded initial gate0/3

Проверены research/16-01-PLAN, текущие auth/session/bootstrap/invitation contracts, schema users/organizations/memberships/sessions, frontend request/store/router, seed_demo и обе nginx-конфигурации. `gsd-tools verify plan-structure`: valid=true,3tasks,errors0,warnings0.

## Основания PASS

- POST `/api/auth/register` создаёт отдельную пустую компанию и owner с серверными UUID. Email/domain/название компании не являются основанием присоединить существующую компанию или скопировать её настройки/авторизацию. Это соответствует multi-tenant архитектуре и явному запросу пользователя.
- План не использует bootstrap-upsert: conflict-email409 и единая транзакция user→org→membership→audit→session защищают существующий аккаунт и не оставляют сирот при отказе. Unique users.email обеспечивает арбитраж конкурентной регистрации, а проверка требует настоящей PG гонки.
- SMTP не становится препятствием к регистрации и не подменяется фиктивным подтверждением почты. Email здесь логин; владение адресом не считается подтверждённым. Приглашение в существующую компанию по-прежнему требует invite token и существующий пароль, если такой email уже занят.
- Typed body/deny_unknown_fields, origin, bounded rate guard перед Argon2 и blocking execution подходят текущему HTTP/DB слою. Me/201 совместим с frontend request: он принимает любой success2xx и стандартный data-envelope.
- Proxy contract реален: host nginx принудительно задаёт X-Forwarded-For=$remote_addr, внутренний nginx передаёт этот заголовок, API опубликован только на loopback/private network. Разбор ровно одного IpAddr и отказ доверять произвольному Forwarded избегает выбора атакующего значения из forwarded-chain.
- Есть необходимые RED/GREEN, realPG rollback/race/isolation и production browser проверки; live-smoke ограничен своей синтетической компанией, исходные компании/секреты/авторизация сохраняются. Phase12 не возобновляется.

## Указания execute/verifier

1. В throttle проверить не только число попыток, но и жёсткий размер map до вставки нового ключа. Поскольку map общий с login, нельзя ожидать длительное signup-window, если login global retain удаляет записи старше15мин: согласовать окно либо раздельное удаление своих prefix. Это реализационная деталь, не изменение scope.
2. Тесты forged headers должны учитывать границу доверия: валидный одиночный XFF принимается только потому, что production ingress его перезаписывает; invalid/multiple IP/Forwarded не должны выбирать произвольный адрес. Не объявлять прямой private API публичным.
3. Новая компания пустая: после автоматического входа проверить не только dashboard200, но и отсутствие старых звонков, подключений и Codex authenticated state. Не связывать реальный Gmail пользователя с прежним demo-owner без отдельного поручения.
4. Статус201 должен выставляться поверх существующего session-response с HttpOnly/Secure/SameSite cookie, не отдельным ответом без cookie. Двойной submit блокируется в UI, а race safety остаётся на сервере.

Это gate качества плана. Реализация, production release и финальный UAT этим документом не объявляются завершёнными.
