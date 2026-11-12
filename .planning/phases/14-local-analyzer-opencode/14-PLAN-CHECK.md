# Независимый PLAN-CHECK — фаза14

Дата: 2026-11-12. Проверяющий: phase14_checker, отдельный агент. Production source не редактировалась.

**PASS. Блокеров плана нет. Revision-loop: 0/3.** Это проверка плана, не готовности туннеля или ASR.

Сверены последняя поправка пользователя про удалённый сайт → Docker-туннель → OpenCode на этом компьютере, требование start-analyzer и два точных model ID, 14-RESEARCH/14-01-PLAN, существующие infra/model-runner/start.mjs/compose.ai.yml и API configuration/readiness. План сохраняет оба запроса, а не заменяет туннель простым запуском дополнительного облачного API.

Различие с доступным Bondscan reference описано честно: старый start-analyzer использует pull удалённой очереди/БД через local SSH forwards, а показанного актуального Docker source здесь нет. Новый Phone reverse HTTP transport обоснован тем, что remote worker хранит записи/БД и вызывает локальный connector. Bondscan credentials, данные и процессы не заимствуются. Отсутствующий SSH target остаётся live gate; локальный настоящий SSH roundtrip обязателен независимо от него.

Topology замкнута: remote worker видит relay по Docker DNS alias model-runner; reverse4096 слушает только внутри relay и не публикуется на host; SSH relay доступен через remote loopback и jump host. Неработающий shortcut remote127.0.0.1→host.docker.internal явно исключён. Два режима Compose и независимый local project позволяют не останавливать существующий стенд.

Контракт ASR корректно отделён от native OpenCode: закреплённая1.18.10 остаётся текстовым session runner, ASR routes реализуются как расширение Docker connector с настоящим STT endpoint. Exact moondream/parakeet-ultra и nvidia/nemotron-3.5-transcribe добавляются в совместимый с pin config, без фиктивного chat inference. Tenant OpenRouter api_key и runtime runner password разделены; наследование ключа при смене провайдера запрещено сервером.

## Обязательные implementation gates

1. **Настоящий Docker SSH roundtrip:** проверить именно remote-worker-container→relay Docker DNS→reverse SSH→local connector, не HTTP fixture без туннеля. Проверить reconnect, неподходящий client key/host key, занятый remote forwarding port и корректную остановку только своего project. SSH control socket без сквозного HTTP не считать доказательством полной готовности.
2. **SSH server feasibility:** non-root sshd, права host keys/authorized_keys и read-only mounts проверить настоящим образом. Если pin OpenSSH требует минимальные дополнительные привилегии, фиксировать конкретное ограничение и повторить независимый review; не включать privileged/host network/Docker socket. Запретить shell/TTY/agent/local forwarding; разрешён remote forward только4096. Host sshd конфигурацию не менять.
3. **Pairing/host-key доверие:** init не перезаписывает существующие ключи/пароли/known_hosts. Публичные ключи различать с приватными; fingerprint remote relay экспортируется явно, StrictHostKeyChecking не ослаблять до accept-new/no. Shared password должен совпадать у server worker и local gateway; ни один «healthy» не объявляется при неверном pairing. Remote target не угадывать из Bondscan source.
4. **Gateway authority:** Basic runner auth проверять до bodyread; tenant header не проксировать в native OpenCode и не помещать в config/env/shared caches. Native backend только loopback4097. ASR upstream фиксированный, redirects запрещены, неизвестные model IDs/URL/control headers отклоняются до отправки аудио. Сырые ошибки upstream/headers/prompts не логируются и не возвращаются.
5. **Bounds/cleanup:** concurrency gate до большого body, лимиты decoded audio и encoded JSON/response, deadline и abort on disconnect. Проверить numeric speaker0/1, timing offsets при chunks, cleanup временных файлов при исключениях/отмене и unknown roles; модель или формат не заменять скрытой второй платной попыткой. Сохранить историю/ручные overrides и usage/null semantics.
6. **API/frontend parity:** provider=opencode для ASR принимает только exact models и operator-owned trusted connector URL. Runtime transport secret никогда не уходит на произвольный URL владельца. Provider-change blank-key guard проверяется direct HTTP, same-provider blank сохраняет прежний ключ. Legacy analysis.opencode auth/UI не менять; test endpoint проверяет именно ASR credentials без inference.
7. **Честная приёмка:** локальные Node/Rust/Vue/PG/SSH/Docker evidence отдельно от реального remote host и платного ASR качества. Финальная инструкция должна различать одноразовое pairing и повседневный start-analyzer, указывать необходимость работающего компьютера. Existing platform/Bondscan containers, auth и настройки не менять автоматически. Фаза12 остаётся paused.

Основные новые регрессии проверить RED без фикса; независимый implementation verifier после execute должен сверить все gates. GSD plan-structure valid4tasks подтверждается root tool output; это не заменяет перечисленных проверок.
