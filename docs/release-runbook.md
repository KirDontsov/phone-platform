# Выпуск

Production: `https://phone.bondscan.ru`, SSH `dks@81.16.141.170`, исходники `/home/dks/phone-platform`. Независимый Compose project `phone-platform`. GitHub: `https://github.com/KirDontsov/phone-platform`, ветка `master`. Сборки Rust/npm используют lockfiles.

Доступы после переноса находятся **только на этом компьютере** в `secrets/production-access.json` (0600), секция `accounts`, основной аккаунт `owner@demo.local`. Известные локальные demo-пароли на публичном сайте отозваны. `secrets/production.env` — отдельная защищённая копия server runtime; ENCRYPTION_KEY сохранён для уже настроенных подключений. Эти файлы не коммитить и не отправлять в чат. Локальный `localhost:18081` — прежняя среда, не production; её writers после cutover остановлены, данные сохранены для отката.

На сервере Mailpit выключен, SMTP пока отсутствует: вход работает, приглашения и сброс пароля по почте требуют собственной SMTP-конфигурации. Расшифровка требует ключа OpenRouter компании в интерфейсе; авторизация Codex не оплачивает OpenRouter.

## Управление установленной версией

В `/home/dks/phone-platform`:

```sh
docker compose --env-file .env.platform --env-file secrets/analyzer-pair.env \
  -f infra/compose.yml -f infra/compose.production.yml \
  -f infra/compose.analyzer-remote.yml -f infra/compose.release.yml ps
```

Те же аргументы используются для `up -d --no-build`, `logs --tail 100 api worker` и `exec`. `PHONE_RELEASE_TAG` выбирает заранее собранные amd64 images. `model-tunnel` healthy только при включённом этом компьютере и локальном `./start-analyzer.sh`; для первоначального запуска API не ждёт relay. Не добавляйте `compose.ai.yml` на сервере: alias `model-runner` принадлежит SSH relay.

Сборка следующей версии из коммита локального Git:

```sh
./scripts/deploy-build.sh dks@81.16.141.170 /home/dks/phone-platform /var/www/phone-platform-build HEAD
```

Скрипт передаёт только tracked source выбранного коммита, собирает пять native linux/amd64 образов отдельным BuildKit с кешем на HDD. Затем выберите созданный тег `15-<commit7>` в приватном server `.env.platform`, проверьте `config --quiet` и выполните `up -d --no-build`. До обновления создайте новый backup/ledger, сохраните старый тег; rollback приложения — вернуть этот тег и `up -d --no-build`, сохраняя БД/media. Миграции БД оцениваются отдельно: переключение images не отменяет схему и не восстанавливает старый backup поверх новых данных. Shared Docker prune запрещён; старые образы и снимки не удаляются автоматически.

Первичный перенос выполняется отдельным `scripts/production-migrate.py`: prepare с явным `--release-tag` → capture с `--hold-writers` → transfer → restore → finalize. Это инструмент **только первого переноса**: непустая target БД/volumes и migration marker запрещают повторный restore. Он сохраняет собственные Phone данные/media/авторизацию Codex, отзывает скопированные web-сессии и временные capabilities. Для обновлений повторять restore нельзя. Приватные snapshots/checksums — `data/backups/production-*` локально и `secrets/migration/production-*` на сервере.

## HTTPS и renew

Отдельный nginx vhost `/etc/nginx/sites-enabled/phone-platform.conf` → `127.0.0.1:18081`, templates `infra/nginx/`. HTTP4096, API, PostgreSQL, RabbitMQ и relay SSH не публикуются наружу. Сертификат `/etc/letsencrypt/live/phone.bondscan.ru/`, Certbot webroot `/var/www/phone-platform-acme`; отдельный domain-scoped renewal hook проверяет nginx и перечитывает сертификат.

На этом сервере sudo без пароля недоступен; для строго ограниченных действий собственного vhost использован одноразовый Docker helper. AppArmor по умолчанию запрещает сигнал host nginx из контейнера, поэтому только этот helper запускается с `apparmor=unconfined`, host PID/network и chroot. App/worker/analyzer эти права не получают:

```sh
docker run --rm --security-opt apparmor=unconfined --network host --pid host \
  --mount type=bind,src=/,dst=/host busybox:1.37 \
  chroot /host /bin/sh /home/dks/phone-platform/scripts/deploy-web.sh check
```

Допустимые действия helper: `http`, `certificate`, `https`, `check`, `renew-test`. Он отказывается менять чужой vhost/hook; nginx-t предшествует reload, при ошибке возвращается собственный прежний файл. `http` — только первоначальный ACME bootstrap, не команда обновления живого сайта. Чужие конфигурации, volumes, учётные записи и секреты Bondscan/Lentaria не меняются.

## Развёртывание с нуля

Локальная поставка запускается `scripts/dev.sh up`, интерфейс — http://localhost:18081. Это независимый Compose project, первоначальный запуск скачивает образы и компилирует приложение.

Для сервера подготовьте отдельную приватную `.env.platform` (0600), `DEMO_MODE=false`, `COOKIE_SECURE=true`, HTTPS `PUBLIC_URL`, точный `ALLOWED_ORIGINS`, рабочую SMTP-конфигурацию и собственные случайные пароли/ENCRYPTION_KEY. Пример структуры создаёт setup-local.py; его demo-значения для сервера заменяются. Ключ шифрования храните отдельно от backup и сохраняйте при обновлении. Телфин/CRM/модели настраиваются владельцем в интерфейсе; ключи моделей runner — в `secrets/model-runner.env`, 0600. Приложение не переносит чужие auth.json.

1. Создайте backup по [инструкции](backup-restore.md), сохраните образы предыдущего выпуска и Git revision.
2. Соберите: `docker compose --env-file .env.platform -f infra/compose.yml -f infra/compose.production.yml build`.
3. Запустите БД/брокер/media-init, затем один API (миграции идемпотентны и сериализуются), затем worker/client: `docker compose --env-file .env.platform -f infra/compose.yml -f infra/compose.production.yml up -d --wait`.
4. Создайте компанию командой `docker compose --env-file .env.platform -f infra/compose.yml run --rm api phone-api bootstrap`; BOOTSTRAP_EMAIL/PASSWORD/NAME/ORGANIZATION задаются только в приватном env. Для следующей компании используйте другое имя; уже существующий аккаунт не получает новый пароль через bootstrap.
5. Проверьте `/ready`, вход, две компании, карточку/Range аудио, задания и свежесть worker heartbeat в «Операции». Затем выполните [UAT](uat.md).

Порты базового Compose доступны только на loopback. TLS reverse proxy направляет запросы на client:18081; API напрямую наружу не публикуется. В production не запускайте Mailpit: укажите внешний SMTP, иначе приглашения/сброс не доставятся.

Перед реальными платными запросами выберите модели, тариф и бюджет в настройках; demo не доказывает качество ASR/AI. Выгрузку «вся история» сверяйте с АТС и её фактическим сроком доступности. Не объявляйте нижнюю границу history_start_verified без этой сверки.

Периодически проверяйте: `/ready`, свежесть worker heartbeat, failed/retry jobs, sync lag/coverage, свободное место media/DB, доставку SMTP и age последнего проверенного backup. «Операции» показывает ошибки и задания; внешняя система оповещений на сервере настраивается оператором.

Проверенный выпуск 12.11.2026: images `15-5789727`, пять собственных образов amd64, PostgreSQL/RabbitMQ и все пять сервисов healthy; HTTPS certificate/renewal dry-run, публичный Python/Chrome smoke и настоящий local analyzer tunnel PASS. Baseline переноса:759calls/614readyrecordings/2organizations, все614readymedia проверены,5encryptedconnections расшифрованы; доступ Codex сохранён. Снимок `data/backups/production-20261112` содержит исходную БД/media/auth и manifest, доступен только локально/оператору. Не запускай `scripts/dev.sh up` на старых локальных томах одновременно с production writers: это снова включит старый worker и скопированную авторизацию Codex. Нужен только `./start-analyzer.sh`.
