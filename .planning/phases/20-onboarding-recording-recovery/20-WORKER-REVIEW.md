# Phase20 independent worker / persistence review

Дата: 2026-11-13. Объём: crates/persistence/src/lib.rs + tests/postgres.rs; phone-worker/src/providers.rs, pipeline.rs + tests/pipeline.rs. READONLY review, production и внешние провайдеры не вызывались.

## Анализ реализации

Замечаний, блокирующих реализацию, не найдено.

- Claim predicate ограничен sync/recording и Telphin connection **той же** organization_id. Другие tenant/kind остаются доступны. Future provider cooldown проверяется до увеличения attempts/fencing.
- SQL не приводит недоверенный JSON text к bigint: type=number, integer regex, верхняя i64 граница и JSON numeric comparison соответствуют worker as_i64 семантике. Missing/null/string/object/array/float/overflow не ломают claim.
- Existing due/state/maxattempt/expired-lease/SKIP LOCKED и finish fencing predicates сохранены. Приоритет действует только среди доступных jobs: purge/retention, затем sync, затем остальные по прежним датам. Future control task не обходит due guard.
- `cached_backoff` — отдельный локальный признак, не берётся из провайдерских payload. Runtime connection guard сохранён на случай изменения cooldown после claim. Cached failure не переписывает deadline, реальный429 использует max(existing,new), не сокращая более поздний cooldown.
- Ready invariant согласован с API: ready=true **или** state=ready; failure не переписывает готовую recording, download success удаляет last_error. Ранний выход до provider config не трогает cached path. Автоматические ручные поля/tenant/media path guard не меняются.
- Нет схемы/массового reschedule/reset attempts/quota bypass. Claim может оставить expired running job ждать cooldown; прежний lease уже недействителен и не может завершить/heartbeat job, затем recovery увеличит fencing штатно.
- Ограничение: между claim и runtime guard в многопроцессном deployment возможна новая свежая429 и одна cached-defer attempt; текущий deployment имеет одного worker. Исправление устраняет основной массовый расход attempts на уже известном cooldown и не отменяет fencing.

## Независимые проверки

Реальный изолированный PostgreSQL16 (синтетическая временная БД): persistence ignored suite **4/4 PASS**, включая все3 новых регрессии, никаких skips. Проверены tenant independence, attempts/fencing preservation, due sync/control priority, max-attempt и malformed JSON.

Первая независимая попытка worker suite столкнулась с загрязнением shared Cargo target от параллельной RED source-copy исполнителя: загруженный binary содержал `pipeline::phase20_red_tests`, отсутствующий в current source, и закономерно упал на исходном cached deadline behavior. Это не зарегистрировано как дефект current implementation. Исполнитель подтвердил причину; требуется очистка только own package build artifacts и повтор current GREEN перед окончательным gate.

Команды: `env TEST_DATABASE_URL=<isolated fixture> cargo test -p persistence --test postgres -- --ignored --test-threads=1`; `env TEST_DATABASE_URL=<isolated fixture> cargo test -p phone-worker --lib --test pipeline --test providers --test model_http -- --test-threads=1`. PG suites запускать последовательно: claim intentionally global across own fixture tenants.

## Повтор после очистки artifact

Исполнитель удалил RED copy/отдельную RED БД и очистил только собственные package artifacts. Независимый последовательный повтор current worktree: persistence **4/4 PASS**, worker lib **20/20 PASS**, pipeline actualPG **8/8 PASS**, provider **16/16 PASS**, model HTTP fixtures **13/13 PASS**. Все 61 проверки реально выполнены, ignored/skipped=0. В current binary правильное имя `pipeline::media_tests::phase20_cached_backoff_does_not_renew_deadline_or_downgrade_ready_media`, test PASS. Заражение shared target устранено; source не менялся для RED.

Статус: **PASS**. Root OAuth/UI review находится отдельно в20-ROOT-REVIEW.md. Production verifier выполнит readonly после release; он не заменяет real provider quota/RouterAI user consent. Reviewer RED мутации самостоятельно не запускал; их evidence предоставляет исполнитель.

## Live verifier дополнение — revision1 требуется

После первого release независимый verifier выявил блокирующий конфликт существующего90day event-age retention с историческим импортом: новый control priority ускорил неявное удаление. Первичный review PASS относится только к узким queue/callback invariants и **не закрывает финальный release gate**. Требуются R1 opt-in/live policy guard/bulk recovery, новый implementation review и повтор production verification. Последствия и evidence зафиксированы20-VERIFICATION.md; worker paused, automatic recovery уже purged данных не разрешён.
