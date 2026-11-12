# Independent plan check — Phase15

Результат: **PASS**, revision-loop **0/3**. Дата:2026-11-12. Независимый проверяющий: `/root/deploy_plan_checker`, автор плана: `/root`.

Проверены research/15-01-PLAN, основной/production/analyzer-remote Compose, auth/bootstrap, migration schema, assistant model_auth/store/runtime и runner storage/auth, snapshot/revoke SQL, client nginx и relay SSH restrictions. `gsd-tools verify plan-structure` вернул valid=true,4tasks,errors0,warnings0.

## Почему план исполним

- Есть явное пользовательское разрешение deployment/push; Git history/master не пересоздаются. Новый независимый Compose project/путь/порты/hostname не требуют ресурсов Bondscan или Lentaria.
- План учитывает реальную архитектуру Macarm64→Linuxamd64, недостаток свободного rootdisk и собственный BuildKit state/output на HDD; не предлагает shared prune.
- Миграция ограничена исходной Phone БД и volumes, проводится при остановленных writers, с пустым target, snapshot/checksum/count verification и rollback. Исходный ENCRYPTION_KEY сохраняется, новые runtime secrets и пароли demo accounts предотвращают публикацию известных доступов.
- Рабочие очереди проверяются до worker start; paid/export операции не выдаются за smoke. Auth volume принадлежит Phone; копирование своего авторизованного runner не требует чужих auth files.
- TLS выпускается для отдельного имени через независимый vhost/webroot; предусмотрены nginx-t, возврат своего файла при ошибке и контроль соседних доменов. Docker maintenance helper допустим в рамках авторизованного deployment, без изменения чужих vhosts/SSH/IAM.
- Реальная pairing-проверка не подменяется fixtures: отдельный restricted key, pinned jump/relay hostkeys, same pair token, remote loopbackSSH и внутренний HTTP4096. Отсутствие реального ASR-key/quality отдельно от healthytransport.
- Финальные independent verification и humanUAT не смешаны с plan-check, SMTP и phase12paused остаются честно видимыми ограничениями.

## Указания для execute/verifier

1. Не применять `ASSISTANT_REVOKE_SQL` из disaster-restore целиком: он обнуляет `assistant_auth_control.authenticated/confirmed_generation` и увеличивает generation, поэтому скопированный runner auth станет недействительным. Для planned transfer отозвать session/capability/pending attempt/run, но сохранить согласованную подтверждённую generation обоих хранилищ и проверить `/assistant/auth/status` обычным read-only API. Публичная авторизация проверяется без inference.
2. Пустоту target проверять до запуска API, поскольку API выполняет миграции при старте. После первого restore повторный запуск migration должен безопасно отказать; обновления выполняются другим documented path, без повторного restore.
3. Source archive и runtime/archive migration разграничить физически. Перед push проверить фактически добавляемые tracked files; `.gitignore` сам по себе не доказывает отсутствие ранних секретов в истории.
4. Root helper должен исполнять подготовленный ограниченный script с фиксированными Phone путями; никакого общего shell-доступа или изменения sudoers. Выпуск/renewal certbot проверять по фактическому результату, не объявлять PASS по наличию cron.
5. Любые рабочие строки, медиа, JSON с credential material передавать через stdin/private600 файлы, не argv/обычные логи. В public smoke artifacts — только safe counts/statuses.

Это проверка качества плана. Deployment, сохранность данных, HTTPS, реальный tunnel и GitHub push в этом документе **не объявляются выполненными**; их доказывает implementation VERIFICATION.
