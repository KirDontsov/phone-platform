# Телфин: автоматическое подключение по доступам АТС

Дата: 2026-11-12. Срез Phase12 research. Выполнены только анонимные GET официальных страниц; вход, регистрация приложения, token grant и запросы к аккаунтам не выполнялись. Phase11 runner tests остаются замороженными на20/20.

## Вывод

Путь без ручного поиска API-ключей существует: backend входит в официальный интерфейс приложений по логину/паролю АТС, регистрирует собственное trusted-приложение с Call API, получает ключи, обменивает их на токен и определяет клиентскую АТС через `/user/`. Успешное выполнение для конкретного аккаунта ещё не доказано. Закрытые формы регистрации/списка/деталей и повторная доступность secret анонимному исследованию недоступны; их нельзя объявлять уже проверенным контрактом.

Официальная [инструкция PDF](https://www.telphin.ru/static/docs/telphin_authorization_manual.pdf) подтверждает использование пароля интерфейса **виртуальной АТС**, создание приложения на `/app/register`, варианты trusted/Call API и выдачу App ID/App Secret. Redirect URL может быть пустым. Это не пароль личного кабинета расчётов по умолчанию: не следует автоматически пробовать разные пользовательские пароли на разных хостах.

## Фактически проверенный анонимный HTTP

Проверка TLS включена. Канонический хост — `https://apiproxy.telphin.ru`, порт443. Последовательность:

```text
GET /app/register
  canonical redirect → /app/register/ (status первого перехода не фиксировался)
GET /app/register/
  302 → /login/?continue_url=https%3A%2F%2Fapiproxy.telphin.ru%2Fapp%2Fregister%2F
GET /login/?continue_url=...
  200 text/html, server-rendered form
```

Первый GET без slash фактически вернул HTML redirect; для адаптера сразу использовать `/app/register/`. Логин-форма содержит:

| Элемент | Подтверждённое значение |
| --- | --- |
| form method | `post` |
| form action | отсутствует; POST на текущий login URL |
| hidden input | `name="_csrf_token"`, `type="hidden"` |
| username | `id="username"`, `name="username"`, `type="text"` |
| password | `id="password"`, `name="password"`, `type="password"` |
| submit | button `Sign In`, без name/value |
| session | `Set-Cookie: session=…; HttpOnly; Path=/` |

В наблюдаемом Set-Cookie не было Secure/SameSite. Наш HTTP-клиент всё равно должен отправлять cookie только этому HTTPS-origin, не доверяя менее строгим атрибутам провайдера. Значения cookie/CSRF не включены в этот файл. На анонимной странице нет MFA/CAPTCHA и скриптов; отсутствие проверки после отправки пароля этим **не подтверждается**. [Официальная страница входа](https://apiproxy.telphin.ru/login/?continue_url=https%3A%2F%2Fapiproxy.telphin.ru%2Fapp%2Fregister%2F).

## Документированные семантика и границы

Уровень **Call API не является read-only**: кроме чтения допускает обработчики событий, удаление записей/сообщений/факсов и управление вызовами. Изменение конфигурации АТС недоступно; All требует администратора. Пользователь read_only ограничивает запросы GET. Trusted наследует пользователя-создателя. Список приложений документирован на `/app/`, но повторная выдача secret не обещана. [Создание приложений и уровни доступа](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901920410).

Токен trusted получают form-urlencoded POST `/oauth/token` с `grant_type=client_credentials`, `client_id=App ID`, `client_secret=App Secret`. OAuth-поле ответа `scope="all"` не следует путать с уровнем приложения All; оно встречается и в документированном trusted grant. [Trusted OAuth](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1981874187).

После grant GET `/api/ver1.0/user/` возвращает в том числе `id`, `client_id`, `extension_group_id`, `extension_id`; это позволяет получить client ID без ввода владельцем. [Официальный пример авторизованного пользователя](https://m.account.telphin.ru/about-company/blog/recommendations/integracija-po-api-chto-jeto-i-dlja-chego-ispolzuetsja). Клиентский, групповой и extension-пользователь имеют разный охват; один успешный login не доказывает доступ ко всей АТС. [Пользователи и полномочия](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901920377).

Есть официальный альтернативный интеграторский OAuth-flow с заранее выданным public-суперприложением и последующим POST `/api/ver1.0/application/` для создания trusted. Он требует собственных реквизитов интегратора и потому не является уже доступным способом bootstrap только из пароля АТС. [Описание интеграторского flow](https://ringme-confluence.atlassian.net/wiki/spaces/Ringme/pages/1901921834).

## Рекомендуемый фиксированный backend adapter

Это проектное решение, а не обещание свойств Телфина.

1. Инструмент `provision_telphin` принимает только encrypted `credential_ref` и обычные поля CAS/idempotency из контракта сервиса. Логин/пароль АТС передаются защищённой формой; MCP/model получает только handle и безопасный progress/result. Ни HTTP URL, ни form fields, ни команды не являются аргументами модели.
2. До внешних действий проверить owner/session/org/run/TTL/ref; взять durable lease/fencing на provision attempt. Backend заранее сохраняет собственный стабильный случайный application marker и состояния attempt. Повторный вызов того же запроса возвращает прежний результат или продолжает reconciliation.
3. Узкий HTTP-клиент: только один официальный HTTPS-host; без userinfo, произвольных портов и IP; отдельный cookie jar в памяти на attempt. Редиректы разбирать вручную с лимитом, разрешая только известные `/login/`, `/app/`, `/app/register/` и подтверждённый путь деталей приложения того же origin. При cross-origin/HTTP/неизвестном redirect — отказ; POST с паролем не пересылать на новый URL через307/308. Login query `continue_url` строить самим, не копировать произвольный URL из HTML.
4. Для login потребовать ровно одну ожидаемую форму, один CSRF, один username/password; reject duplicate controls/CSRF/form ambiguity, неизвестные обязательные поля и опасный action. Сохранить cookie от GET и отправить CSRF в том же jar. Не исполнять JS и не использовать браузерный MCP. Повторный login после отказа не циклический; никаких reset-password или смены настроек аккаунта.
5. После входа прочитать `/app/`, затем registration form. Имя собственного приложения определяется backend marker. Тип выбрать по option trusted, доступ — Call API; Redirect URL пустой. Названия HTML inputs/select option values **пока неизвестны**, их нельзя угадать и отправлять слепым POST: parser должен проверить известные semantic labels, допустимые controls и единственный match; незнакомая схема → `provider_form_changed` до create. Реальный sanitized authenticated fixture нужен для закрепления точных field names.
6. Перед create сверить список приложений с own journal. Не брать чужое приложение по похожему имени и не переиспользовать существующее только по совпадению названия. Проверять стабильный own marker, известный external app ID и аккаунт, если они уже зафиксированы. Не удалять и не менять существующие приложения.
7. Перед единственным registration POST зафиксировать `create_attempted`. Из успешного ответа немедленно извлечь пару ID/secret в строго ожидаемых полях/подписях, запечатать и записать durable checkpoint; не возвращать HTML/secret в сообщения, ошибки или audit. Далее token grant и `/user/`, client-level scope check и существующий connection service/CAS. Секрет подключения сохранять до удаления одноразового credential handle. Доступы АТС после успеха уничтожать; зафиксировать границу хранения, если reconciliation ещё требует их.
8. Даже при auth-success перед финальным local commit перепроверять исходную authority/revision/fencing. При потерянной authority удалённое приложение могло уже появиться: сообщать orphan/uncertain, не утверждать rollback и не создавать новый app на следующем запросе. Не держать общую DB transaction открытой во время сетевых запросов.

## Неопределённый результат и защита от дубликатов

Регистрация не имеет документированного idempotency key. Timeout/EOF после отправки POST означает `uncertain`, а не разрешение retry. Сначала GET `/app/` и только собственного matching app detail. Если ID и secret доступны — сохранить их и продолжить. Если приложение видно, а secret повторно не выдаётся, автоматическое восстановление недоказуемо: сохранить `reconciliation_required`, не выполнять blind recreate, rotate/delete или выбор случайного app. Даже пустой список после timeout сам по себе не доказывает отсутствие запоздалого создания; отложенная проверка/консервативный отказ безопаснее второго POST. AppSecret one-shot встречается в сторонних инструкциях, но официального подтверждения не найдено: адаптер обязан считать повторную доступность неизвестной.

Стабильное имя должно укладываться в maxlength реальной формы; не выводить из него логин/компанию/номер. Совпадение marker без совпадения own journal/account не доказывает владение. При нескольких matches — отказ. Создание внешнего приложения и локальная transaction не атомарны; durable state machine/checkpoints нужны даже при working HTTP.

## MFA, ограничения и необходимые проверки

Если после POST пароля появляется OTP/CAPTCHA/consent/неизвестная форма, возвращать безопасный `interaction_required`/`provider_form_changed`, не обходить защиту и не спрашивать OTP в обычном чате. Live-права/способ подтверждения заранее неизвестны. Неверные доступы, ограниченные права и MFA должны иметь разные безопасные состояния, где это возможно определить без публикации raw HTML.

Для plan/verifier нужны offline HTML+HTTP fixtures: настоящий login layout с синтетическим CSRF; успешная регистрация/однократный secret; ошибочный пароль; MFA; duplicate CSRF/controls; unknown form; evil action/redirect; отсутствующий trusted/Call API; cookie isolation; API user group/extension scope; concurrent/replayed requests; response loss после remote create; видимый app без secret; crash после create до checkpoint; CAS/authority revoke; encrypted output/audit redaction. Offline fixtures не доказывают, что закрытая текущая HTML-форма Телфина совпадает с ними. Отдельный live gate — только с пользовательскими доступами АТС и явной фактической проверкой, без обещания заранее.

Повторные HTTP failures исследовательских источников: прямой curl PDF вернул403, `m.account.telphin.ru` не прошёл TLS hostname validation; TLS не отключалась. Для фактов использованы доступные официальные PDF/web-index и Confluence. Эти результаты не являются проблемами исследуемого login-origin, который по HTTPS GET доступен.
