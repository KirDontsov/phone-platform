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

## Revision1 — live retention incident

**План revision1 PASS, 1/3. Финальный release/UAT gate BLOCKED до реализации и проверки R1.**

Первичная автоматическая проверка не выявила конфликт существующего retention(default90days по started_at) с импортом исторических звонков. После первого выпуска приоритет purge ускорил существующее неявное удаление: 24648 звонков помечены deleted,120 уже purged,24528 ещё целы. Единственный worker остановлен root через pause. Эти последствия требуют исправления и отдельного доказательства восстановления, а не только прежнего green suite.

R1 добавляет явный owner opt-in `organization.data.retention_enabled=true`, default=false, типизированный API/UI с понятным предупреждением о возрасте звонка и историческом импорте. Семантика started_at не меняется скрытно. Retention и каждый retention-origin purge проверяют LIVE enabled/currentdays перед unlink; ручной purge сохраняется. Неистёкший/disabled retention-origin job может снять только soft-delete, если нет purged/tombstone/manual audit/manual purge proof.

Устранён blocker revision: до resume выполняется scoped bulk reconcile всех доказанных pending retention deletions одной guarded transaction с private snapshot/manifest/receipt и dry-run/idempotence actualPG. Иначе24528 no-op purges заняли бы часы, сохраняя скрытую историю и starvation sync. Cancel только queued/retry retention-origin jobs без active lease, restore только целые softdeleted calls той же компании без ручных удалений/тombstone/purged. Уже purged120 не восстанавливаются из пустого placeholder; их bounded source recovery требует реальных данных и отдельного gate.

Проверки R1 обязательны: defaultoff/fresh historical import сохраняет записи, expliciton по текущему age работает, выключение/изменение days после постановки блокирует unlink, manualdelete/tombstone сохраняются, malformed policy не включает очистку, bulk reconcile точно scoped и идемпотентен. Перед resume безопасный новый образ, независимый review и readonly production verifier. План structure повторно valid=true,4tasks,errors/warnings=[]; provider requests operator не выполняет до отдельного scope.
