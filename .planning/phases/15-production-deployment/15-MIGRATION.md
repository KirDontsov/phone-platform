# Planned Phone migration: implementation evidence

Дата:2026-11-12. Исполнитель `/root/production_migration`. Только Phone Platform; Bondscan/Lentaria runtime и данные не использовались. Это доказательство taskB, не общая приёмка phase15.

## Реальный перенос

- Подготовлены собственные production runtime secrets, исходный Phone ENCRYPTION_KEY сохранён. HTTPS PUBLIC_URL/точный ALLOWED_ORIGINS, COOKIE_SECURE=true, DEMO_MODE=false, Mailpit выключен. Независимый SMTP пока не предоставлен; явно недоступен127.0.0.1:1.
- Snapshot `data/backups/production-20261112` создан с `capture --hold-writers`: API/worker/assistant-runner исходного стенда остановлены и остаются остановлены до cutover/rollback. Очереди jobs/outbox/assistant_runs были0; незавершённые платные/экспортные действия не переносились в работающую очередь.
- Baseline:2организации,6users,6memberships,27471documents,1247jobs,759calls,614recordings/ready media,10connections. Сверены полные counts и digest содержимого всех connection/call/recording/transcript/analysis документов после restore.
- Скопированы только собственные DB/media/assistant_auth. Проверены SHA-256 всех трёх архивов на обеих сторонах; restored ready media614/614 существуют.5encrypted config реально расшифрованы AES256GCM исходным ключом в памяти, результат — только count, без provider requests/секретов в логах.
- Полностью сохранены собственный Codex auth volume и confirmed generation контрольной записи. Скопированные web sessions/token grants отозваны, временные assistant credentials удалены, capabilities/session hashes старых turns отозваны; постоянные подключения не менялись.
- Только3известных seeded demo accounts получили разные случайные production passwords. Все6users и memberships сохранены. Новые пароли только в локальном `secrets/production-access.json`0600, схема `{"url":"https://phone.bondscan.ru","accounts":{"email":"password"}}`; remote получает только PHC Argon2id hashes по SSH stdin.
- Новая целевая БД проверена пустой ДО API; повторный restore блокируется marker/непустой схемой. Новые volumes проверены пустыми. Никаких DROP/clean/перезаписи существующего production restore в utility нет.
- Из-за медленного upload использованы4SSH streams/16MiB chunks, максимум64MiB памяти. SHA-256+size перед atomic publish, completed chunks удалены. Единственный legacy incomplete file был подтверждён как неполный и удалён после остановки собственного transfer PID; исходные архивы сохранены.

## Реальный private HTTP smoke

API поднят и healthy на server loopback18080.3старых публичных demo passwords отвергнуты401;3новых аккаунта вошли200; Secure/HttpOnly/SameSite=Lax cookies3/3, demo_modefalse,2компании, unauthenticated calls401. Все созданные probe sessions разлогинены. Никаких model/Telphin/CRM calls ради smoke.

Найден и исправлен production runtime bug: unquoted inline YAML tmpfs разбивался на2mount specs, Docker не создавал API. Исправлены обе строки API/worker (`"/tmp:size=64m,mode=1777"`,512m). Проверено реальным runtime API и тестом настоящего Compose parser. Второй runtime guard: только bind-mounted PUBLIC analyzer key644 для UID10001; все приватные файлы600. Настоящий Docker UID10001 read probe прошёл без вывода ключа.

## Проверки

`scripts/tests/test_production_migrate.py`:20/20PASS, включая настоящий отдельный PostgreSQL cleanup fixture (после теста удалён), Node GCM/tag test и actual Compose YAML parsing без project env. Восемь RED mutation checks подтверждены в изолированных module copies/fixtures: targetempty guard, premature source restart, Codex auth reset, partial upload hash guard, joined archive hash guard, public key mode600, resume при работающем production writer, прежний unquoted tmpfs. Активный исходный код не мутировался.

Приватные агрегированные отчёты: `data/eval/phase15-migration.json`, `phase15-private-login.json`, `phase15-migration-red.json`. Target marker `secrets/production-migrated.json`0600; source archives/runtime/access вне Git.

## Cutover completed / передача root

Root проверил публичный HTTPS smoke, tenant isolation/Range206, Codex authenticated=true, настоящий remote→local Docker tunnel/native1.18.10+catalog и desktop/mobile browser. Все5release images15-5789727 подняты healthy. Private runtime tag изменён отдельно, существующие ключи/пароли/ENCRYPTION_KEY сохранены.

`python3 scripts/production-migrate.py finalize --directory data/backups/production-20261112` выполнен после healthy worker/API: исходный persistent digest повторно совпал со snapshot, production healthchecks прошли. Все3исходных writers(api/worker/assistant-runner) подтверждены exited; исходные DB/media/auth volumes и snapshot сохранены для rollback. Артефакт `data/eval/phase15-cutover.json`.

Root владеет общей implementation VERIFICATION/UAT и GitHub push. Данный документ не выдаёт сохранность auth/transport за live inference: конечная ASR/model quality, независимый SMTP и финальная пользовательская приёмка остаются отдельно. `resume-source` требует сначала остановить production writers и поднимет только исходно работавшие сервисы из manifest.
