# UAT14 — локальный analyzer

## Автоматические результаты

- Main platform API/worker/client/model-runner пересобраны и healthy; Codexassistant/DB/broker/media сохранены. Standalone `./start-analyzer.sh local` выполнен на этом компьютере: projectphone-analyzer/model-runner healthy, localhost18496; ownkeys/pairing/template созданы0600, SSH не запускался из-за отсутствующего remotehost.
- Genuine isolated Docker SSH: remote-worker container→relayDNS→jump+reverseSSH→local newgateway→nativeOpenCode1.18.10 health иASRcatalogдвухмоделей PASS. Anonymous/shell/wrongclientkey/wronghostkey/pairingmismatch/forwardportconflict negatives иrestart/reconnect/credentialreload PASS. Это реальный транспорт, но synthetic SSHhosts, не сервер пользователя.
- Node connector17 PASS наhost ивhardenedDocker; nativemetadata/config/sessioncleanup/MCP0/tool-deny PASS. Worker45=lib14+HTTP8+PGpipeline7+providers16 PASS; API/PG3 PASS. Pythonstartup10 PASS; frontend75/typecheck/build PASS. Основныеnew regression RED; дополнительныеguards не все отдельно мутировались — summaries/VERIFICATION содержат точную границу.
- Реальный Docker browser read-only: обе модели внастройках, fixedreadonlyURL, tenantOpenRouterlabel, mobile390безoverflow, legacyanalysisfreeformunchanged, JSerrors0/configwrites0/modelrequests0. Проверочнаясессияlogout. Ниодна реальная запись вэтой фазе не отправлялась модели.

## Одноразовая проверка пользователя

1. Укажи host/user собственного сервера в `.env.analyzer.local`; адрес пока не предоставлен. Ключи/пароли вчат не нужны.
2. Выполни `./start-analyzer.sh remote-instructions`: отдельный Phonepublickey разрешается наjumpSSH, server overlayподнимаетprivateDockerrelay, pairingфайлпередаётсязащищённо. Проверь fingerprints и закрепи server/relaypublickeys командойpin. Не копируй учётные записи/секреты Bondscan.
3. `./start-analyzer.sh` → `./start-analyzer.sh doctor`: готовыlocalconnector и сквозной HTTPчерезSSH. Остановка толькосвоегоruntime: `./start-analyzer.sh down`.
4. В настройках удалённого сайта: «Расшифровка → OpenCode(Docker)→OpenRouter», выбери moondream/parakeet-ultra либоnvidia/nemotron-3.5-transcribe, введи ключ OpenRouter компании. Адрес model-runner остаётсявнутреннимDockeralias.
5. Сохрани и проверь соединение. Затем отправьоднуготовуюзапись; убедисьвтексте/метках/ошибках/usage. Войдя внастройкудругойкомпании, чужойключувидетьнельзя. Роли0/1неравныавтоматическименеджеру/клиенту, при необходимости назначьихвкарточке.
6. Остановилноутбук/туннель: серверпоказываетзадержку/retry, неfake success; послевосстановленияобработкапродолжается. Точнаяживаяполитикаповтора ифактическаятарификацияпроверяютсянаконтрольномзвонке.

Подробности: [docs/local-analyzer.md](../../../docs/local-analyzer.md).

## Открытые gates

Реальный serverSSHhost/pairing не проверены, посколькуадрес/доступнепредоставлены. Реальная OpenRouterавторизация/модельнаядоступность, поддержкаtimestamps/диаризациинааккаунте, качествоASRиролей/стоимость остаютсяpending. АвторизацияCodex/OpenCodeнеподключаетOpenRouterAPIавтоматически. Фаза12paused, finalhumanUATpending, внешниеacceptedcriteriaнеобъявленыPASS.
