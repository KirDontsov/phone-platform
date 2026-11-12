# Independent implementation review — Phase15

Проверяющий: `/root/deploy_plan_checker`. Дата:2026-11-12. Scope: deploy-web.sh, infra/nginx/*, deploy-build.sh, compose.release.yml, smoke-production.py; migration source прочитан для проверки связей, исправления выполняет его автор.

## Findings

1. **Medium, privacy:** новый host nginx `error_log ... warn` может сохранять полную строку запроса с query при upstream errors. В query встречаются phone search/filter и reset/invite links. Access log уже безопасно исключает query; error log следует привести к `crit`, как в client nginx. Перед открытием проверить фактический установленный файл.
2. **Medium, cutover:** исходный capture возобновляет localworker сразу после снимка, а finalize позже требует неизменный digest call/recording/connection/transcript/analysis. Рабочий импорт уже продолжался во время исследования. Если localworker продолжит работу между capture и finalize, получится ожидаемый отказ завершения после запуска remote. Требуется явная остановка localworker сразу после capture/через cutover или сохранение quiesce до успеха с возвратом state при ошибке. Автоматический обход digest недопустим; повторный restore поверх target тоже недопустим.

Findings переданы root до публикации HTTPS; статус исправлений/фактическая проверка добавляются после применения.

Повторная проверка: **оба findings исправлены**. Finding1: source теперь `error_log ... crit`, root сообщил о передаче файла на сервер до HTTPS. Finding2: реализованы `capture --hold-writers`, возобновление прежних writers в finally при неуспехе capture, явный `resume-source` для rollback, finalize оставляет все3старых writers выключенными. Реальный manifest `production-20261112` содержит writers_held_for_cutover=true, quiesced api/worker/assistant-runner,759calls/614recordings(все614ready),2org/6users. Независимый read-only Docker ps подтвердил все3writers остановлены. Digest guard не отключён.

Независимый повторный прогон15/15tests PASS, включая настоящий PostgreSQL cleanup в новой disposable БД `phone_migration_test_*`; БД удалена в finally. Проверены сохранение confirmedauth/generation и connections, отзыв sessions/grants/capabilities, безопасный target guard, private files/archives, hold/failure-restart. RED-artifact автора `data/eval/phase15-migration-red.json`: все3мутации (emptyguard/restart/authreset) вызывают failures; source mutations самим reviewer повторно не выполнялись. Certbot renewal log также прочитан: simulated renewal success для собственного phone certificate.

## Проверено без замечаний

- Shell parse: `bash -n scripts/deploy-build.sh`, `sh -n scripts/deploy-web.sh`; `python3 -m py_compile scripts/smoke-production.py` PASS.
- BuildKit pinned digest, native amd64, HDD state/output, fixed own containername, CPU/RAM limit, build lock, disk-space floors, no shared prune and no external BuildKit port. Source archive создаётся из Git commit, runtime отдельно; build report не содержит секретов.
- HTTPS helper меняет фиксированный Phone vhost, защищает от unmanaged site/symlink, nginx-t до reload, rollback прежнего собственного файла при ошибке. Certbot отдельное имя/webroot, renewal deploy hook ограничен Phone domain. Существующие vhosts и SSH глобальные настройки не переписываются.
- Основной nginx proxy поддерживает buffering off, SSE timeouts, Range forwarding по стандартному nginx proxy; app/API остаются loopback, private runner не публикуется.
- Production smoke читает credentials только из private regular600 file, проверяет точный HTTPS target и системный truststore. Пароли не в argv/логах. Проверяются demo denial/newlogin, Secure/HttpOnly/SameSite cookie, actual tenant guard и crossorg404, ready-recording Range206, сохранённый assistant auth без inference. Customer bodies/ids не печатаются; own sessions закрываются.
- Auth generation contract проверен в model_auth.rs и runner auth/storage: сохранение подтверждения и согласованной generation необходимо; migration source не применяет destructive disaster revoke.
- Release overlay сохраняет Dockerfiles для build metadata, но operational startup должен использовать `--no-build`; image tags неизменяемые по revision. First migration защищает пустой target до API start.

Это source review, не final VERIFICATION. HTTPS/application/data/tunnel/renewal/GitHub оцениваются по фактическим deployed smoke evidence отдельно.
