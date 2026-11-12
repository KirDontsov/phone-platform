# Phase17 — независимая проверка реализации и выпуска

Дата: 2026-11-12. Проверяющий: timestamp_plan_checker; автор реализации и выпуска — root.

**Статус: implementation/release PASS; live completion PARTIAL — внешняя квота Телфина.** Исправление, выпуск, настоящий импорт и штатный backoff подтверждены. Завершение исходного sync и восстановление configured/polling ещё не проверены: API потребовал длительную паузу. Фаза не объявляется полностью принятой пользователем.

## Код и регрессии — PASS

Независимые PLAN-CHECK и REVIEW проведены до выпуска. Actual Worker::sync использует sync_range; пустые optional dates больше не вызывают invalid_provider_timestamp. Строгий provider/checkpoint parser, диапазон, UTC и tenant/fencing/CAS не менялись. Без доказанной нижней границы coverage остаётся false. Дополнение error→configured выполняется только в successful final_write и сохраняет disconnected/configured.

Независимо выполнены пять новых sync_range tests:5/5 PASS. Просмотрены RED и GREEN evidence: две исходные blankdate регрессии падают без fix, три контрольных сценария проходят. GREEN workerlib/providers/modelHTTP:42 фактически выполненных проверки PASS, один существующий PG-case пропущен без TEST_DATABASE_URL (Cargo показывает43ok, это не43 реально выполненных). Для трёхстрочного восстановления state отдельный RED не выполнялся; требуемое evidence — настоящий успешный sync/следующий polling.

## Выпуск и сохранность — PASS

Runtime15-2087a86, code2087a864d0e3e217bd3e7bdd9042d31bfd5955b5; предварительный b238453 не активировался. Native build evidence содержит все пять linux/amd64 images. Проверяющий самостоятельно прочитал docker ps на разрешённом сервере:8 Phone services healthy, все пять appimages ожидаемого тега. GitHub master на момент проверки равен code2087a864; stash phase12 ddddf25f4ef384b0a02359042efa4bee159a1f09 сохранён.

Просмотрен operational script: retry — единственный guarded UPDATE исходного jobs row по org/job/subject/kind/failed/error и непереключённому connection; fencing/lease/attempts соответствуют существующему retry_job. Receipt retried_count=1; никакого нового outbox/importjob не создаётся. Проверяющий выполнял только readonly check, не retry/snapshot и не provider/model calls; секреты не читал. Полная сохранность произвольных изменяемых пользователем данных отдельным backup/restore не тестировалась, обновление не содержит миграций/restore/delete/ротации.

## Live — импорт/backoff PASS, завершение pending внешней квоты

Независимые настоящие readonly PostgreSQL checks: история последовательно выросла 0→11944→28103→41560→43370 уникальных calls. Последний checkpoint window33/page7, imported43371 (overlap/dedup), coverage_complete=false. После успешных страниц Telphin вернул provider_rate_limited: job retry/attempt1, run retry_pending, connection.last_error=provider_rate_limited. Дополнительный независимый readonly SELECT next_attempt_at показал retry_in_seconds=3218 (около54мин), lease отсутствует: это сохранённая длительная пауза провайдера, а не потерянное задание. Частый опрос прекращён до её истечения; операторский повтор/перезапуск worker/обход квоты не выполнялся. Штатный automatic retry с checkpoint ожидается; исходной invalid_provider_timestamp больше нет. Старая connection.state=error очищается только после успешного final_write; итоговые completed/configured/last_error=null/новый scheduled polling **не проверены**.

Postdeploy smoke PASS: три старых demo passwords отклонены, три текущих operator login успешны, Secure cookie, organization guards/foreigncall denial, media Range, скрытые encrypted config, assistant ready/authenticated. Analyzer doctor подтверждает реальный SSH/HTTP roundtrip/pairing и два local healthy containers. Публичная Chrome-проверка registration/обязательного OTP/invalid400+me401/query stripping/1440+390 PASS, overflow0/js_errors0, register submits0/emails0. Это evidence просмотрено проверяющим; эти сценарии повторно самостоятельно не запускались. Smoke provider_requests=0/configuration_writes=0; реальные чтения Telphin выполняет исходный импорт пользователя.

## Границы

Финальный пользовательский UAT остаётся pending (17-UAT.md). Последние364дня без verified lower bound не доказывают полноту всей истории API. Качество ASR/анализа/CRM/Sheets не проверяется этой фазой. Чеклист Григория не проверялся: нет чеклиста.

Для последующей автоматической проверки достаточно readonly `python3 data/eval/phase17-sync-operator.py check` после provider backoff; повторный retry не нужен. Требуемые оставшиеся факты: исходный job succeeded, соответствующий run completed, connection configured/last_error=null и хотя бы один новый scheduled sync после retry. До их появления эти пункты нельзя выдавать за PASS.
