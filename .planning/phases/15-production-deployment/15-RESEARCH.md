# Phase 15: production deployment / GitHub / real analyzer pairing

Дата: 2026-11-12. Пользователь явно разрешил push существующего проекта в KirDontsov/phone-platform (master), deployment на dks@81.16.141.170 и phone.bondscan.ru, пример lentaria.bondscan.ru. Историю Git и README сохраняем.

## Проверенные источники

- SSH BatchMode работает; Git HTTPS ls-remote возвращает success и пустой новый репозиторий (osxkeychain). gh auth отсутствует, но не нужен при рабочем Git credential helper.
- DNS phone/lentaria: CNAME bondscan.ru →81.16.141.170. Lentaria: /home/dks/lentaria, nginx /etc/nginx/sites-enabled/lentaria.conf →127.0.0.1:9140, отдельный Certbot сертификат. Phone сертификата нет, default vhost показывает чужой просроченный cert.
- Сервер x86_64,8CPU,25GB available RAM; rootdisk16GB free, DockerRootDir /var/lib/docker. /var/www→/mnt/hdd4tb/www:1.9TB free, www-data group writable. Не prune shared cache/images. Собственный BuildKit state/output на HDD, runtime images в Docker; native remote build предпочтительнее медленной QEMU Rust-компиляции на Macarm64.
- dks имеет Docker, но sudo-n требует пароль. Для строго своей nginx-конфигурации/Certbot/reload возможен одноразовый Docker maintenance helper; никаких изменений других vhosts, ключей или процессов. Existing SSH account прямо передан пользователем.
- Порты18080/18081/25437/25672/18422 свободны; target /home/dks/phone-platform отсутствует. Own Compose project phone-platform.
- Локальная Phone БД содержит обе компании, реальные Телфин настройки/звонки, assistant auth volume пользователя. 3 demo.local пользователей с членством, 3 example.test без членства. Нельзя публиковать известные demo passwords. Отдельный новый production password для каждого demo account хранить private local secrets/production-access.json, remote hashes только. Чужих реальных пользовательских аккаунтов сейчас нет.
- Собственная .env.platform содержит ключ шифрования постоянных подключений; его требуется сохранить при миграции. .env/auth/БД/ключи Bondscan не читались.
- Phase14 реализовал реальный Docker relay и local start-analyzer. Теперь известны jump host/user; нужны собственный pubkey restricted permitopen, pinned public hostkeys и тот же pairing runtime.

## Решения / границы

Переносить только Phone DB/media/собственный assistant_auth в защищённом SSH канале после остановки writers, сравнить aggregates/checksums; revoke web sessions/grants и незавершённые assistant runs/capabilities. В отличие от disaster restore перенести собственный auth volume с проверенной авторизацией, не брать host/Bondscan auth. Сохранить credentials encryption key. Образы для amd64 собрать отдельным HDD-backed ephemeral BuildKit, не очищать общие ресурсы. До публичного открытия проверить login/tenant/media/worker и запрет старого demo password. Новые случайные production DB/broker/token secrets, DEMO_MODE=false, Secure cookies, exact HTTPS origins. SMTP не предоставлен: не маскировать недоступность invite/reset; не подключать чужой SMTP/Mailpit публично.

ACME: собственный webroot/новый HTTP vhost, certbot webroot phone.bondscan.ru, затем HTTPS vhost и nginx-t/reload с откатом только своего файла при ошибке. Проверять работающие соседние домены до/после без изменений. Host maintenance helper запускается лишь для собственных конфигов и стандартной проверки/reload; не расширять SSH/IAM.

Нет платных model/ASR/Telphin/CRM вызовов ради проверки. Проверка настоящего tunnel/native health/catalog без inference. Финальный пользовательский UAT отдельно. Фаза12 paused/stash сохранены.
