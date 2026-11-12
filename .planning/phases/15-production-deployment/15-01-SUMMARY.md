# Phase15 implementation summary

Research → независимый PLAN-CHECK PASS0/3 → execute → независимое review/revision → verifier → automatedUAT. Code release:5789727; appbaseline2e894a4, Git history/master сохранены, origin KirDontsov/phone-platform подключён, codepushed. Фаза12/stash не возобновлялись.

## Реализация четырёх задач

- A: native linux/amd64 api/worker/client/assistant/relay images, pinned BuildKitv0.34.2, отдельный HDDstate/cache/output,4CPU/12GBquota, source-only archives, immutable SHA context, staging lock, disk guards. Общий кеш/образы не очищались; после build собственные output tar удалены, cache сохранён. Холодный ffmpeg apt на HDD занял24m26s; повторные сборки используют cache. `scripts/deploy-build.sh` и `check-deploy-build.py` обеспечивают повторяемость.
- B: защищённый planned transfer только Phone, source writers held. Baseline2orgs/6users/6memberships/759calls/614readyrecordings/10connections/27471documents/1247jobs; checksums/digest до cleanup совпали,614readyfiles существуют,5encryptedconfig успешно расшифрованы. Собственные Codex auth volume и confirmedgeneration сохранены; websessions/grants/capabilities отозваны, временные assistantcredentials удалены. Три известных demo passwords заменены отдельными случайными; plaintext только в local0600 accessfile, новые runtime DB/broker/assistant secrets, прежний encryptionkey. Первичный restore требует пустых targetDB/volumes и отказывается повторяться; перенос архива atomic+checksum, bounded4SSHstreams ускорили медленную сеть. Snapshot защищён и сохранён для rollback.
- C: отдельные nginx vhost/webroot/LEcertificate phone.bondscan.ru до2027-02-10, HTTP→HTTPS, query-safe logs, renewal dry-runPASS. Host nginx и другие сайты сохранены. Собственный restricted Phone SSHpubkey/hostkeypins, private relay127.0.0.1:18422/internal4096, localDockeranalyzer; настоящий remoteDocker→relay→этот компьютер→OpenCode1.18.10/catalog двух моделей PASS. Anonymous/wrongpassword401; inference0.
- D: публичные Python/Chrome smokes PASS:3oldpassword401/new3login, Secure/HttpOnly/Lax, demo_modefalse,2org/header409/foreigncall404, mediaRange206, authenticatedCodexstatus, desktop1440/mobile390/JS0/overflow0/no settings writes. Runtime images release15-5789727, allserviceshealthy. Docs/update/rollback/access/UAT подготовлены; source-only GitHub публикация без runtime secrets.

## Исправления по фактическому запуску

Независимые review/fixes: nginxerrorlogcrit; удержание sourcewriters черезcutover; bindPUBLICkey644 для UID10001 при privatefiles600; quotedtmpfs (иначе YAML создавал лишний mount); tar-xpf сохраняет gitmodes при umask077 и defensive runtime code permissions/non-root assertions в relay/assistant Dockerfiles. Не chmod весь project/secrets. Реальный первый relay image с masked permissions дал PermissionDenied; исправленный image прошёл non-root compile/read и реальный SSH health.

20migrationtests PASS включая отдельный PostgreSQL и настоящий Compose parser;8RED mutations проверены. NativeLinux staging/modes/immutablecontext/corruptarchive/flock PASS, без tar-p тест падает. Полные applicationRust/UI suites заново не запускались: application code API/worker/client не менялся, Docker client typecheck/Vite и native release builds прошли, реальный deployed browser/API smoke покрывает новую среду. NativeCodex0.167.2/OpenCode1.18.10 версионные и non-root проверки прошли.

## Границы

HumanUAT pending. Живые model inference/ASRquality, SMTP и CRM/Sheets accounts не выдаются заPASS. Компьютер/Docker/analyzer должны работать для модельного транспорта. Worker после cutover может продолжать штатный импорт; baseline сравнивался до его запуска. Старые локальные writers остановлены, данные/backup сохранены. Подробности independentVERIFICATION и15-UAT.md.
