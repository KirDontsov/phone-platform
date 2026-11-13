# Phase20 independent PLAN-CHECK

Результат: **PASS**, revision 0/3, 2026-11-13.

Проверены AGENTS.md, STATE/PROJECT/ROADMAP, 20-RESEARCH.md, 20-01-PLAN.md, текущие claim_job/finish_job, worker connection_config/record_failure и RouterAI consent callback. План состоит из трёх исполнимых задач: callback recovery, очередь/состояния записей, review/release/UAT. `gsd-tools verify plan-structure` — valid=true, errors=[], warnings=[], task_count=3.

## Покрытие требований

- Пустой callback: конкретная регрессия установлена в коде, но причина пользовательской пустой страницы не доказана; план сохраняет эту границу. Непустая безопасная страница, очистка query, сообщение конкретного enum и ограниченное время обслуживания проверяются настоящим HTTP fixture.
- OAuth: exact Host/state/issuer/TTL/S256 и однократный exchange сохранены; no replay при ambiguous timeout, приватная tenant-bound установка остаётся прежней. Успех определяется сохранённым runtime, не одним нажатием кнопки. Внешний consent остаётся human gate.
- Очередь: пропуск собственного future cooldown до claim защищает бюджет попыток; приоритет due sync/retention/purge устраняет выявленное вытеснение массовыми recording. Сохранены fencing/lease/max attempts и работа независимой компании.
- Проверки: новый SQL проверяется настоящим изолированным PostgreSQL, включая malformed JSON, expired lease, чужую компанию; ключевые регрессии проходят RED→GREEN. UI/API ready precedence не маскируется общим состоянием подключения.
- Выпуск: собственные сервисы, idle-worker guard, no schema migration/reset attempts/forced retry/quota bypass. Авторизация и медиа сохраняются. Внешняя квота и пользовательская проверка прежних конкретных записей явно не объявляются закрытыми локальными тестами.

## Замечания исполнителю, не блокирующие план

1. Provider predicate применять только к sync/recording и connection того же organization_id. Не использовать небезопасный cast arbitrary JSON -> bigint; тип и пределы проверять до cast либо сравнивать JSON numeric безопасным способом.
2. Приоритет — перед backlog только среди due jobs; next_attempt_at/lease ограничения обязательны. Не заставлять sync работать во время cooldown и не понижать retention/purge. По одному worker сейчас нет конкурентного race, но claim не заменяет runtime connection_config guard.
3. Ошибочный callback не должен менять успешный/активный flow. Чистая ошибка не должна отражать полученные query/body/URL и вызывать повторный exchange; не ослаблять issuer ради неизвестной причины.
4. Не обещать восстановление конкретных вчерашних записей без идентификации и проверки: есть aggregate evidence ready files, а не доказательство по названному пользователем звонку.

PLAN-CHECK относится только к качеству плана. Реализация, release и actual user consent здесь не проверены.

Уточнение исполнителя учтено: callback ошибки сохраняют HTTP400/409/410 и readable HTML с немедленным meta Refresh на фиксированный allowlisted `/problem/<status>`; чужой Host не перенаправляется, ответ не отражает query. Success остаётся303 `/connected`. Consent default600s и success grace60s ограничены и подходят плану. Проверять HTTP тело/meta target, clean page и отсутствие параметров; окончательная ошибка exchange должна оставаться видимой на странице результата.
