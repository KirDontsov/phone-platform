# Выпуск

Локальная поставка запускается `scripts/dev.sh up`, интерфейс — http://localhost:18081. Это независимый Compose project `phone-platform`; никаких данных Bondscan не требуется. Сборки Rust/npm используют lockfiles. На первом запуске скачивание образов и компиляция могут занять значительное время.

Для сервера подготовьте отдельную приватную `.env.platform` (0600), `DEMO_MODE=false`, `COOKIE_SECURE=true`, HTTPS `PUBLIC_URL`, точный `ALLOWED_ORIGINS`, рабочую SMTP-конфигурацию и собственные случайные пароли/ENCRYPTION_KEY. Пример структуры создаёт setup-local.py; его demo-значения для сервера заменяются. Ключ шифрования храните отдельно от backup и сохраняйте при обновлении. Телфин/CRM/модели настраиваются владельцем в интерфейсе; ключи моделей runner — в `secrets/model-runner.env`, 0600. Приложение не переносит чужие auth.json.

1. Создайте backup по [инструкции](backup-restore.md), сохраните образы предыдущего выпуска и Git revision.
2. Соберите: `docker compose --env-file .env.platform -f infra/compose.yml -f infra/compose.production.yml build`.
3. Запустите БД/брокер/media-init, затем один API (миграции идемпотентны и сериализуются), затем worker/client: `docker compose --env-file .env.platform -f infra/compose.yml -f infra/compose.production.yml up -d --wait`.
4. Создайте компанию командой `docker compose --env-file .env.platform -f infra/compose.yml run --rm api phone-api bootstrap`; BOOTSTRAP_EMAIL/PASSWORD/NAME/ORGANIZATION задаются только в приватном env. Для следующей компании используйте другое имя; уже существующий аккаунт не получает новый пароль через bootstrap.
5. Проверьте `/ready`, вход, две компании, карточку/Range аудио, задания и свежесть worker heartbeat в «Операции». Затем выполните [UAT](uat.md).

Порты базового Compose доступны только на loopback. TLS reverse proxy оператора должен направлять запросы на client:18081; API напрямую наружу не публикуется. В production не запускайте Mailpit: укажите внешний SMTP, иначе приглашения/сброс не доставятся. Файл compose.production.yml — настройки запуска на выбранном сервере, публикация на внешний сервер в этой работе не выполнялась.

Перед реальными платными запросами выберите модели, тариф и бюджет в настройках; demo не доказывает качество ASR/AI. Выгрузку «вся история» сверяйте с АТС и её фактическим сроком доступности. Не объявляйте нижнюю границу history_start_verified без этой сверки.

Периодически проверяйте: `/ready`, свежесть worker heartbeat, failed/retry jobs, sync lag/coverage, свободное место media/DB, доставку SMTP и age последнего проверенного backup. «Операции» показывает ошибки и задания; внешняя система оповещений на сервере настраивается оператором.
